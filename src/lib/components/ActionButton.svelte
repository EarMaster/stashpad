<script lang="ts">
    import type { Snippet } from "svelte";
    import { twMerge } from "tailwind-merge";
    import { tooltip } from "$lib/actions/tooltip";

    let {
        variant = "additional",
        danger = false,
        active = false,
        class: className = "",
        onclick,
        title,
        disabled = false,
        children,
        ...rest
    } = $props<{
        variant?:
            | "instant"
            | "additional"
            | "main"
            | "complete"
            | "drag"
            | "context"
            | "top";
        danger?: boolean;
        active?: boolean;
        class?: string;
        onclick?: (e: MouseEvent) => void;
        title?: string;
        disabled?: boolean;
        children: Snippet;
        [key: string]: any;
    }>();

    const baseClass =
        "rounded transition-all flex items-center justify-center cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed";

    const variants = {
        // Always visible, subtle (e.g., Add File, Copy)
        instant:
            "p-1 hover:bg-muted text-muted-foreground/50 hover:text-foreground",

        // Standard actions (e.g., Edit, Move)
        additional:
            "p-1.5 hover:bg-muted text-muted-foreground hover:text-foreground",

        // Prominent actions (e.g., Save/Stash)
        main: "px-3 py-1.5 bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm font-medium text-xs",

        // Toggle actions (e.g. Complete)
        complete: "h-7 w-7 p-0 rounded-md",

        // Drag handles
        drag: "h-7 w-7 p-0 rounded-md cursor-grab active:cursor-grabbing shrink-0",

        // Context actions - collapse to icon on small screens
        context:
            "group px-2 py-1 hover:bg-muted text-muted-foreground hover:text-foreground text-xs rounded flex items-center gap-1.5 transition-colors",

        // Small badge-style buttons (e.g., version toggle)
        top: "gap-1 px-1.5 py-0.5 min-h-5 text-[10px] bg-secondary/50 text-muted-foreground border-border hover:bg-secondary hover:text-foreground",
    };

    // Active state styling for badge variant
    let activeStyles = $derived(
        active && variant === "top"
            ? "bg-primary/10 text-primary border-primary/20"
            : "",
    );
</script>

<button
    class={twMerge(
        baseClass,
        variants[variant],
        activeStyles,
        danger && "hover:bg-red-500/10 hover:text-red-500",
        className,
    )}
    {onclick}
    {title}
    use:tooltip
    {disabled}
    type="button"
    {...rest}
>
    {@render children()}
</button>
