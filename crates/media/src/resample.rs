//! Streaming sample-rate conversion (windowed sinc, Blackman window).
//!
//! Speech recognition wants 16 kHz; sources are mostly 44.1/48 kHz (or 8 kHz
//! phone audio). The filter low-passes below the lower Nyquist frequency, so
//! downsampling does not alias. The kernel is tabulated once (oversampled and
//! linearly interpolated), which keeps the cost at ~80 multiply-adds per output
//! sample for 48 → 16 kHz.

/// Zero crossings of the sinc kernel on each side (quality vs. speed).
const ZERO_CROSSINGS: f64 = 12.0;
/// Pass band as a fraction of the lower Nyquist frequency.
const ROLLOFF: f64 = 0.92;
/// Kernel table entries per input sample.
const OVERSAMPLE: usize = 256;

/// Converts a mono stream from one rate to another, chunk by chunk.
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Half the kernel width, in input samples.
    radius: usize,
    table: Vec<f32>,
    /// Unconsumed input, with `radius` samples of history in front.
    buf: Vec<f32>,
    /// Position of the next output sample in `buf`.
    pos: f64,
    passthrough: bool,
    total_in: u64,
    total_out: u64,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let step = f64::from(in_rate.max(1)) / f64::from(out_rate.max(1));
        // Cut-off in cycles per input sample.
        let fc = 0.5 * (1.0 / step).min(1.0) * ROLLOFF;
        let radius_f = ZERO_CROSSINGS / (2.0 * fc);
        let radius = radius_f.ceil() as usize;
        let table = (0..=radius * OVERSAMPLE + 1)
            .map(|i| {
                let x = i as f64 / OVERSAMPLE as f64;
                if x >= radius_f {
                    return 0.0;
                }
                let arg = 2.0 * fc * x;
                let sinc = if arg == 0.0 {
                    1.0
                } else {
                    (std::f64::consts::PI * arg).sin() / (std::f64::consts::PI * arg)
                };
                let u = x / radius_f;
                let window = 0.42
                    + 0.5 * (std::f64::consts::PI * u).cos()
                    + 0.08 * (2.0 * std::f64::consts::PI * u).cos();
                (2.0 * fc * sinc * window) as f32
            })
            .collect();
        Self {
            step,
            radius,
            table,
            buf: vec![0.0; radius],
            pos: radius as f64,
            passthrough: in_rate == out_rate,
            total_in: 0,
            total_out: 0,
        }
    }

    /// Feed `input`, appending every output sample that can be computed so far.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.total_in += input.len() as u64;
        if self.passthrough {
            out.extend_from_slice(input);
            self.total_out += input.len() as u64;
            return;
        }
        self.buf.extend_from_slice(input);
        self.run(out);
    }

    /// End of input: flush what is left (the tail is filtered against silence).
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if self.passthrough {
            return;
        }
        let expected = (self.total_in as f64 / self.step).round() as u64;
        let before = out.len();
        self.buf
            .extend(std::iter::repeat_n(0.0, self.radius * 2 + 2));
        self.run(out);
        let extra = self.total_out.saturating_sub(expected) as usize;
        let keep = out.len().saturating_sub(extra).max(before);
        out.truncate(keep);
        self.total_out = self.total_out.min(expected);
    }

    fn run(&mut self, out: &mut Vec<f32>) {
        let r = self.radius;
        while (self.pos.floor() as usize) + r < self.buf.len() {
            let center = self.pos.floor() as usize;
            let frac = self.pos - center as f64;
            let mut acc = 0.0f32;
            // Taps k in (center - r, center + r]; distance |k - pos|.
            for k in (center + 1).saturating_sub(r)..=center + r {
                let distance = (k as f64 - center as f64 - frac).abs();
                acc += self.buf[k] * self.kernel(distance);
            }
            out.push(acc);
            self.total_out += 1;
            self.pos += self.step;
        }
        // Drop input no future output sample can reach.
        let keep_from = (self.pos.floor() as usize).saturating_sub(r);
        if keep_from > 0 {
            self.buf.drain(..keep_from.min(self.buf.len()));
            self.pos -= keep_from as f64;
        }
    }

    fn kernel(&self, distance: f64) -> f32 {
        let t = distance * OVERSAMPLE as f64;
        let i = t.floor() as usize;
        let (Some(&a), Some(&b)) = (self.table.get(i), self.table.get(i + 1)) else {
            return 0.0;
        };
        let frac = (t - i as f64) as f32;
        a + (b - a) * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let n = (f64::from(rate) * seconds) as usize;
        (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin() as f32)
            .collect()
    }

    /// Amplitude of `freq` in `samples` (single-bin DFT).
    fn amplitude(samples: &[f32], freq: f64, rate: u32) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &s) in samples.iter().enumerate() {
            let phase = 2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate);
            re += f64::from(s) * phase.cos();
            im += f64::from(s) * phase.sin();
        }
        2.0 * (re * re + im * im).sqrt() / samples.len() as f64
    }

    fn resample_in_chunks(input: &[f32], from: u32, to: u32, chunk: usize) -> Vec<f32> {
        let mut r = Resampler::new(from, to);
        let mut out = Vec::new();
        for piece in input.chunks(chunk) {
            r.push(piece, &mut out);
        }
        r.finish(&mut out);
        out
    }

    #[test]
    fn keeps_length_and_pitch() {
        for from in [8_000, 22_050, 44_100, 48_000] {
            let input = sine(440.0, from, 1.0);
            let out = resample_in_chunks(&input, from, 16_000, 1000);
            assert_eq!(out.len(), 16_000, "{from} Hz");
            // Skip the edges (the filter ramps in and out).
            let middle = &out[2000..14_000];
            let a = amplitude(middle, 440.0, 16_000);
            assert!((a - 1.0).abs() < 0.05, "{from} Hz: amplitude {a}");
            assert!(
                amplitude(middle, 1000.0, 16_000) < 0.02,
                "{from} Hz: no other tone"
            );
        }
    }

    #[test]
    fn removes_frequencies_above_the_new_nyquist() {
        // 12 kHz would alias to 4 kHz at 16 kHz.
        let input = sine(12_000.0, 48_000, 0.5);
        let out = resample_in_chunks(&input, 48_000, 16_000, 4096);
        let middle = &out[1000..7000];
        assert!(amplitude(middle, 4000.0, 16_000) < 0.01);
    }

    #[test]
    fn chunking_does_not_change_the_result() {
        let input = sine(300.0, 44_100, 0.3);
        let a = resample_in_chunks(&input, 44_100, 16_000, 7);
        let b = resample_in_chunks(&input, 44_100, 16_000, 100_000);
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6));
    }

    #[test]
    fn same_rate_passes_through() {
        let input = sine(300.0, 16_000, 0.1);
        assert_eq!(resample_in_chunks(&input, 16_000, 16_000, 33), input);
        assert!(resample_in_chunks(&[], 48_000, 16_000, 10).is_empty());
    }
}
