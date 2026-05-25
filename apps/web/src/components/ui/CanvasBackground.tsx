import { useEffect, useRef } from "react";

interface Particle {
  x: number;
  y: number;
  r: number;
  baseOpacity: number;
  opacity: number;
  pulseSpeed: number;
  pulsePhase: number;
  vx: number;
  vy: number;
}

interface ShootingStar {
  x: number;
  y: number;
  vx: number;
  vy: number;
  length: number;
  opacity: number;
  life: number;
  maxLife: number;
}

interface CanvasBackgroundProps {
  intensity?: "landing" | "moderate" | "subtle" | "barely" | "hidden";
}

export function CanvasBackground({ intensity = "moderate" }: CanvasBackgroundProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const mouseRef = useRef({ x: 0, y: 0, targetX: 0, targetY: 0 });

  useEffect(() => {
    if (intensity === "hidden") return;

    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animationFrameId: number;
    let particles: Particle[] = [];
    let shootingStar: ShootingStar | null = null;
    let width = (canvas.width = window.innerWidth);
    let height = (canvas.height = window.innerHeight);

    // Dynamic thresholds by intensity context
    const maxParticles =
      intensity === "landing" ? 75 : intensity === "moderate" ? 50 : intensity === "subtle" ? 30 : 15;
    const maxOpacityMultiplier =
      intensity === "landing" ? 1.0 : intensity === "moderate" ? 0.7 : intensity === "subtle" ? 0.4 : 0.2;
    const connectionMaxDist = 120;

    // Build particle deck
    const initParticles = () => {
      particles = [];
      for (let i = 0; i < maxParticles; i++) {
        particles.push({
          x: Math.random() * width,
          y: Math.random() * height,
          r: Math.random() * 1.2 + 0.4,
          baseOpacity: (Math.random() * 0.12 + 0.03) * maxOpacityMultiplier,
          opacity: 0,
          pulseSpeed: Math.random() * 0.01 + 0.005,
          pulsePhase: Math.random() * Math.PI * 2,
          vx: (Math.random() - 0.5) * 0.05,
          vy: (Math.random() - 0.5) * 0.05,
        });
      }
    };

    initParticles();

    // Resize boundaries
    const handleResize = () => {
      width = canvas.width = window.innerWidth;
      height = canvas.height = window.innerHeight;
      initParticles();
    };
    window.addEventListener("resize", handleResize);

    // Track smooth mouse movements for parallax
    const handleMouseMove = (e: MouseEvent) => {
      const scale = intensity === "landing" ? 3.0 : 1.5;
      mouseRef.current.targetX = ((e.clientX - width / 2) / (width / 2)) * scale;
      mouseRef.current.targetY = ((e.clientY - height / 2) / (height / 2)) * scale;
    };
    window.addEventListener("mousemove", handleMouseMove);

    // Shooting star trigger clock: fires every 20-45s
    let lastShootingStarTime = Date.now() + Math.random() * 15000;
    const triggerShootingStar = () => {
      const angle = Math.PI / 4 + (Math.random() - 0.5) * 0.15; // diagonal motion
      const speed = Math.random() * 6 + 6;
      shootingStar = {
        x: Math.random() * (width * 0.6),
        y: 0,
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed,
        length: Math.random() * 60 + 50,
        opacity: Math.random() * 0.1 + 0.15,
        life: 0,
        maxLife: Math.random() * 40 + 30,
      };
      lastShootingStarTime = Date.now();
    };

    // Cap updates at ~30 FPS to minimize rendering pipeline overhead
    let lastFrameTime = 0;
    const render = (time: number) => {
      animationFrameId = requestAnimationFrame(render);

      // Throttling duration check
      const delta = time - lastFrameTime;
      if (delta < 33) return; // ~30fps lock
      lastFrameTime = time;

      ctx.clearRect(0, 0, width, height);

      // Interpolate smooth mouse parallax transition
      const mouse = mouseRef.current;
      mouse.x += (mouse.targetX - mouse.x) * 0.05;
      mouse.y += (mouse.targetY - mouse.y) * 0.05;

      // Draw faint connections (constellations)
      if (intensity !== "barely") {
        ctx.strokeStyle = "rgba(108, 99, 255, 0.015)";
        ctx.lineWidth = 0.5;
        for (let i = 0; i < particles.length; i++) {
          const p1 = particles[i];
          for (let j = i + 1; j < particles.length; j++) {
            const p2 = particles[j];
            // Apply mouse parallax offset to coordinate computations
            const p1x = p1.x - mouse.x * p1.r * 1.5;
            const p1y = p1.y - mouse.y * p1.r * 1.5;
            const p2x = p2.x - mouse.x * p2.r * 1.5;
            const p2y = p2.y - mouse.y * p2.r * 1.5;

            const dx = p1x - p2x;
            const dy = p1y - p2y;
            const dist = Math.sqrt(dx * dx + dy * dy);

            if (dist < connectionMaxDist) {
              ctx.beginPath();
              ctx.moveTo(p1x, p1y);
              ctx.lineTo(p2x, p2y);
              ctx.stroke();
            }
          }
        }
      }

      // Draw active particles
      for (const p of particles) {
        // Slow float updates
        p.x += p.vx;
        p.y += p.vy;

        // Bounce edges
        if (p.x < 0 || p.x > width) p.vx *= -1;
        if (p.y < 0 || p.y > height) p.vy *= -1;

        // Slow pulsing luminance phase updates
        p.pulsePhase += p.pulseSpeed;
        p.opacity = p.baseOpacity * (1.0 + Math.sin(p.pulsePhase) * 0.35);

        // Render star
        const px = p.x - mouse.x * p.r * 1.5;
        const py = p.y - mouse.y * p.r * 1.5;

        ctx.fillStyle = `rgba(226, 232, 240, ${p.opacity.toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(px, py, p.r, 0, Math.PI * 2);
        ctx.fill();
      }

      // Draw rare shooting stars
      if (shootingStar) {
        const star = shootingStar;
        star.x += star.vx;
        star.y += star.vy;
        star.life += 1;

        const alpha =
          star.life < star.maxLife * 0.2
            ? (star.life / (star.maxLife * 0.2)) * star.opacity
            : (1 - star.life / star.maxLife) * star.opacity;

        ctx.strokeStyle = `rgba(108, 99, 255, ${alpha.toFixed(3)})`;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(star.x, star.y);
        ctx.lineTo(star.x - star.vx * 2, star.y - star.vy * 2);
        ctx.stroke();

        if (star.life >= star.maxLife) {
          shootingStar = null;
        }
      } else {
        // Periodically verify if we should launch a new shooting star
        const now = Date.now();
        const durationSinceLast = now - lastShootingStarTime;
        if (durationSinceLast > 25000 && Math.random() < 0.005) {
          triggerShootingStar();
        }
      }
    };

    render(0);

    return () => {
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("mousemove", handleMouseMove);
      cancelAnimationFrame(animationFrameId);
    };
  }, [intensity]);

  if (intensity === "hidden") return null;

  return (
    <canvas
      ref={canvasRef}
      className="fixed inset-0 -z-10 pointer-events-none bg-[#0a0b0f] block"
      style={{ mixBlendMode: "screen" }}
    />
  );
}
