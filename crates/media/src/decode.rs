//! Audio decoding to 16 kHz mono `f32`, streamed packet by packet.
//!
//! Containers and codecs (pure Rust): WAV, MP3, FLAC, Ogg (Vorbis, Opus), MP4/M4A/MOV
//! (AAC, ALAC), WebM/Matroska (Vorbis, Opus). Video tracks are ignored: the first
//! audio track we can decode is used. Opus goes through `opus-decoder`, which
//! decodes straight to 16 kHz mono; everything else through Symphonia and
//! [`Resampler`].

use std::time::Duration;

use opus_decoder::OpusDecoder;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_NULL, CODEC_TYPE_OPUS, CodecParameters, DecoderOptions},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    probe::Hint,
};

use crate::{MediaError, resample::Resampler};

/// The sample rate everything is converted to (what Whisper expects).
pub const SAMPLE_RATE: u32 = 16_000;

/// After this many undecodable packets in a row the track counts as damaged.
const MAX_BAD_PACKETS: u32 = 100;
/// Largest Opus frame: 120 ms at 16 kHz, mono.
const OPUS_MAX_FRAME: usize = 1920;

enum Codec {
    Symphonia {
        decoder: Box<dyn symphonia::core::codecs::Decoder>,
        buffer: Option<SampleBuffer<f32>>,
        resampler: Option<Resampler>,
    },
    Opus {
        decoder: Box<OpusDecoder>,
        frame: Vec<f32>,
        /// Encoder delay still to drop, in output samples.
        skip: usize,
    },
}

/// Reads one audio track as 16 kHz mono samples.
pub struct AudioReader {
    format: Box<dyn FormatReader>,
    track_id: u32,
    codec: Codec,
    /// Decoded, converted samples not handed out yet.
    pending: Vec<f32>,
    /// Scratch for one packet's mono samples before resampling.
    mono: Vec<f32>,
    done: bool,
    bad_packets: u32,
    duration: Option<Duration>,
}

impl AudioReader {
    /// Open `source` (stored as `mime`) and pick its first decodable audio track.
    pub fn open(source: Box<dyn MediaSource>, mime: &str) -> Result<Self, MediaError> {
        let mut hint = Hint::new();
        let mime = mime.split(';').next().unwrap_or_default().trim();
        hint.mime_type(mime);
        if let Some(ext) = extension_for(mime) {
            hint.with_extension(ext);
        }
        let stream = MediaSourceStream::new(source, Default::default());
        let options = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let probed = symphonia::default::get_probe()
            .format(&hint, stream, &options, &MetadataOptions::default())
            .map_err(|e| {
                unsupported("the audio could not be read (unknown or damaged format)", e)
            })?;
        let format = probed.format;

        let codecs = symphonia::default::get_codecs();
        let audio: Vec<&CodecParameters> = format
            .tracks()
            .iter()
            .map(|t| &t.codec_params)
            .filter(|p| p.codec != CODEC_TYPE_NULL)
            .collect();
        let track = format
            .tracks()
            .iter()
            .find(|t| {
                let codec = t.codec_params.codec;
                codec == CODEC_TYPE_OPUS
                    || (codec != CODEC_TYPE_NULL && codecs.get_codec(codec).is_some())
            })
            .ok_or_else(|| {
                if audio.is_empty() {
                    MediaError::Unsupported("this file has no audio track".into())
                } else {
                    MediaError::Unsupported("this file's audio codec is not supported".into())
                }
            })?;
        let params = track.codec_params.clone();
        let track_id = track.id;
        let duration = duration_of(&params);

        let codec = if params.codec == CODEC_TYPE_OPUS {
            let channels = params.channels.map_or(1, |c| c.count());
            if channels > 2 {
                return Err(MediaError::Unsupported(
                    "Opus audio with more than two channels is not supported".into(),
                ));
            }
            let decoder = OpusDecoder::new(SAMPLE_RATE, 1)
                .map_err(|e| unsupported("the Opus audio could not be decoded", e))?;
            Codec::Opus {
                decoder: Box::new(decoder),
                frame: vec![0.0; OPUS_MAX_FRAME],
                // Pre-skip is counted at 48 kHz.
                skip: params.delay.unwrap_or(0) as usize / 3,
            }
        } else {
            let decoder = codecs
                .make(&params, &DecoderOptions::default())
                .map_err(|e| unsupported("this file's audio codec is not supported", e))?;
            Codec::Symphonia {
                decoder,
                buffer: None,
                resampler: None,
            }
        };
        Ok(Self {
            format,
            track_id,
            codec,
            pending: Vec::new(),
            mono: Vec::new(),
            done: false,
            bad_packets: 0,
            duration,
        })
    }

    /// The track's length, when the container says.
    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Up to `max` more samples; empty at the end of the track.
    pub fn read(&mut self, max: usize) -> Result<Vec<f32>, MediaError> {
        while self.pending.len() < max && !self.done {
            self.decode_packet()?;
        }
        let n = max.min(self.pending.len());
        Ok(self.pending.drain(..n).collect())
    }

    fn decode_packet(&mut self) -> Result<(), MediaError> {
        let packet = match self.format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return self.finish();
            }
            // A new chained stream (rare): stop at the end of the first.
            Err(SymphoniaError::ResetRequired) => return self.finish(),
            Err(SymphoniaError::IoError(e)) => return Err(MediaError::Io(e.to_string())),
            Err(e) => {
                tracing::debug!(error = %e, "demuxing stopped");
                return self.finish();
            }
        };
        if packet.track_id() != self.track_id {
            return Ok(());
        }
        let decoded = match &mut self.codec {
            Codec::Opus {
                decoder,
                frame,
                skip,
            } => match decoder.decode_float(&packet.data, frame, false) {
                Ok(n) => {
                    let n = n.min(frame.len());
                    let drop = (*skip).min(n);
                    *skip -= drop;
                    self.pending.extend_from_slice(&frame[drop..n]);
                    true
                }
                Err(e) => {
                    tracing::debug!(error = %e, "bad Opus packet");
                    false
                }
            },
            Codec::Symphonia {
                decoder,
                buffer,
                resampler,
            } => match decoder.decode(&packet) {
                Ok(audio) => {
                    let spec = *audio.spec();
                    let channels = spec.channels.count().max(1);
                    let frames = audio.capacity() as u64;
                    if buffer
                        .as_ref()
                        .is_none_or(|b| b.capacity() < audio.capacity() * channels)
                    {
                        *buffer = Some(SampleBuffer::new(frames, spec));
                    }
                    if let Some(buffer) = buffer.as_mut() {
                        buffer.copy_interleaved_ref(audio);
                        self.mono.clear();
                        self.mono.extend(
                            buffer
                                .samples()
                                .chunks(channels)
                                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
                        );
                    }
                    let resampler =
                        resampler.get_or_insert_with(|| Resampler::new(spec.rate, SAMPLE_RATE));
                    resampler.push(&self.mono, &mut self.pending);
                    true
                }
                Err(SymphoniaError::DecodeError(e)) => {
                    tracing::debug!(error = e, "bad audio packet");
                    false
                }
                Err(SymphoniaError::IoError(e)) => return Err(MediaError::Io(e.to_string())),
                Err(e) => return Err(unsupported("the audio could not be decoded", e)),
            },
        };
        if decoded {
            self.bad_packets = 0;
        } else {
            self.bad_packets += 1;
            if self.bad_packets > MAX_BAD_PACKETS {
                return Err(MediaError::Unsupported("the audio track is damaged".into()));
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), MediaError> {
        self.done = true;
        if let Codec::Symphonia {
            resampler: Some(r), ..
        } = &mut self.codec
        {
            r.finish(&mut self.pending);
        }
        Ok(())
    }
}

fn unsupported(message: &str, detail: impl std::fmt::Display) -> MediaError {
    tracing::debug!(%detail, "{message}");
    MediaError::Unsupported(message.to_owned())
}

fn duration_of(params: &CodecParameters) -> Option<Duration> {
    let frames = params.n_frames?;
    if let Some(tb) = params.time_base {
        let t = tb.calc_time(frames);
        return Some(Duration::from_secs(t.seconds) + Duration::from_secs_f64(t.frac));
    }
    let rate = params.sample_rate?;
    Some(Duration::from_secs_f64(
        frames as f64 / f64::from(rate.max(1)),
    ))
}

/// A file extension Symphonia's probe recognises for a stored MIME type.
fn extension_for(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/mp4" | "audio/m4a" => "m4a",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/flac" | "audio/x-flac" => "flac",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" | "audio/webm" => "webm",
        _ => return None,
    })
}

/// Decode all of `bytes` (tests and small files).
pub fn decode_all(bytes: Vec<u8>, mime: &str) -> Result<Vec<f32>, MediaError> {
    let mut reader = AudioReader::open(Box::new(std::io::Cursor::new(bytes)), mime)?;
    let mut all = Vec::new();
    loop {
        let piece = reader.read(SAMPLE_RATE as usize * 10)?;
        if piece.is_empty() {
            return Ok(all);
        }
        all.extend(piece);
    }
}
