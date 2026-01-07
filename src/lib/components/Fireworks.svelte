<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
-->

<script lang="ts">
    import { onMount, onDestroy } from "svelte";
    import { fade } from "svelte/transition";

    /**
     * Props for controlling the fireworks display.
     */
    let { show = $bindable(false), duration = 4000 } = $props<{
        show?: boolean;
        duration?: number;
    }>();

    let canvas = $state<HTMLCanvasElement>();
    let ctx: CanvasRenderingContext2D | null = null;
    let animationFrameId: number | null = null;
    let particles: Particle[] = [];
    let startTime = 0;

    /**
     * Particle class for firework effects.
     */
    class Particle {
        x: number;
        y: number;
        vx: number;
        vy: number;
        alpha: number;
        color: string;
        gravity: number;
        friction: number;
        size: number;

        constructor(
            x: number,
            y: number,
            vx: number,
            vy: number,
            color: string,
        ) {
            this.x = x;
            this.y = y;
            this.vx = vx;
            this.vy = vy;
            this.alpha = 1;
            this.color = color;
            this.gravity = 0.15;
            this.friction = 0.98;
            this.size = Math.random() * 3 + 2;
        }

        /**
         * Update particle position and physics.
         */
        update() {
            this.vy += this.gravity;
            this.vx *= this.friction;
            this.vy *= this.friction;

            this.x += this.vx;
            this.y += this.vy;

            this.alpha -= 0.01;
        }

        /**
         * Draw the particle on canvas.
         */
        draw(ctx: CanvasRenderingContext2D) {
            ctx.save();
            ctx.globalAlpha = this.alpha;
            ctx.fillStyle = this.color;
            ctx.beginPath();
            ctx.arc(this.x, this.y, this.size, 0, Math.PI * 2);
            ctx.fill();
            ctx.restore();
        }

        /**
         * Check if particle is still alive.
         */
        isDead() {
            return this.alpha <= 0;
        }
    }

    /**
     * Create a firework explosion at the given position.
     */
    function createFirework(x: number, y: number) {
        const particleCount = 50 + Math.random() * 50;
        const colors = [
            "hsl(var(--primary))",
            "hsl(45, 100%, 60%)", // Gold
            "hsl(280, 100%, 70%)", // Purple
            "hsl(160, 100%, 60%)", // Cyan
            "hsl(10, 100%, 65%)", // Red
            "hsl(200, 100%, 65%)", // Blue
        ];
        const color = colors[Math.floor(Math.random() * colors.length)];

        for (let i = 0; i < particleCount; i++) {
            const angle = (Math.PI * 2 * i) / particleCount;
            const speed = Math.random() * 6 + 2;
            const vx = Math.cos(angle) * speed;
            const vy = Math.sin(angle) * speed;

            particles.push(new Particle(x, y, vx, vy, color));
        }
    }

    /**
     * Animation loop for rendering fireworks.
     */
    function animate(timestamp: number) {
        if (!ctx || !canvas) return;

        if (startTime === 0) {
            startTime = timestamp;
        }

        const elapsed = timestamp - startTime;

        // Stop after duration
        if (elapsed >= duration) {
            show = false;
            return;
        }

        // Clear canvas completely for transparency
        ctx.clearRect(0, 0, canvas.width, canvas.height);

        // Create new fireworks periodically
        if (Math.random() < 0.03) {
            const x = Math.random() * canvas.width;
            const y =
                Math.random() * (canvas.height * 0.6) + canvas.height * 0.1;
            createFirework(x, y);
        }

        // Update and draw particles
        particles = particles.filter((particle) => {
            particle.update();
            particle.draw(ctx!);
            return !particle.isDead();
        });

        animationFrameId = requestAnimationFrame(animate);
    }

    /**
     * Initialize canvas and start animation.
     */
    function startFireworks() {
        if (!canvas) return;

        ctx = canvas.getContext("2d");
        if (!ctx) return;

        // Set canvas size
        canvas.width = window.innerWidth;
        canvas.height = window.innerHeight;

        particles = [];
        startTime = 0;

        // Create initial burst
        for (let i = 0; i < 3; i++) {
            setTimeout(() => {
                if (!canvas) return;
                const x = Math.random() * canvas.width;
                const y =
                    Math.random() * (canvas.height * 0.5) + canvas.height * 0.1;
                createFirework(x, y);
            }, i * 200);
        }

        animationFrameId = requestAnimationFrame(animate);
    }

    /**
     * Stop animation and cleanup.
     */
    function stopFireworks() {
        if (animationFrameId !== null) {
            cancelAnimationFrame(animationFrameId);
            animationFrameId = null;
        }
        particles = [];
        startTime = 0;
    }

    /**
     * Handle window resize.
     */
    function handleResize() {
        if (canvas && show) {
            canvas.width = window.innerWidth;
            canvas.height = window.innerHeight;
        }
    }

    // Watch for show state changes
    $effect(() => {
        if (show) {
            startFireworks();
        } else {
            stopFireworks();
        }
    });

    onMount(() => {
        window.addEventListener("resize", handleResize);
    });

    onDestroy(() => {
        stopFireworks();
        window.removeEventListener("resize", handleResize);
    });
</script>

{#if show}
    <div
        class="fixed inset-0 z-50 pointer-events-none"
        transition:fade={{ duration: 300 }}
    >
        <canvas
            bind:this={canvas}
            class="w-full h-full"
            style="mix-blend-mode: screen;"
        ></canvas>
    </div>
{/if}
