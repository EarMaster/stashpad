<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { portal } from "$lib/actions/portal";
    import { trapFocus } from "$lib/actions/trapFocus";

    let {
        open = $bindable(false),
        title,
        description,
        confirmText,
        cancelText,
        variant = "default",
        onConfirm,
        onCancel,
    } = $props<{
        open: boolean;
        title: string;
        description: string;
        confirmText?: string;
        cancelText?: string;
        variant?: "default" | "destructive";
        onConfirm: () => void;
        onCancel?: () => void;
    }>();

    let isConfirmed = $state(false);
    let confirmBtn = $state<HTMLButtonElement | null>(null);
    let hasOpened = $state(false);

    // Several of these exist on screen at once - Queue alone mounts three - so the
    // ids that tie the panel to its heading have to be per instance.
    const uid = $props.id();
    const titleId = `${uid}-title`;
    const descriptionId = `${uid}-description`;

    let finalConfirmText = $derived(confirmText || $_("common.save"));
    let finalCancelText = $derived(cancelText || $_("common.cancel"));

    // Reset confirmed state when dialog opens
    $effect(() => {
        if (open) {
            isConfirmed = false;
            hasOpened = true;
        }
    });

    // Ensure parent state is synchronized when dialog is closed/dismissed
    $effect(() => {
        if (!open && !isConfirmed && hasOpened) {
            onCancel?.();
        }
    });

    // The confirm button is the default target, as it was under bits-ui. `trapFocus`
    // has already put focus on Cancel by the time this runs, so this moves it on.
    $effect(() => {
        if (open) confirmBtn?.focus();
    });

    function handleKeydown(event: KeyboardEvent) {
        if (open && event.key === "Escape") {
            event.preventDefault();
            handleCancel();
        }
    }

    function handleCancel() {
        open = false;
    }

    function handleConfirm() {
        isConfirmed = true;
        onConfirm();
        // Do not force open = false here. Rely on onConfirm to update parent state,
        // which will update the 'open' prop reactively.
        // If parent doesn't update open, we might want to force it, but for now
        // let's assume parent controls it to avoid state desync.
        // open = false;
    }
</script>

<svelte:window onkeydown={handleKeydown} />

<!--
  A plain `{#if}` overlay rather than a bits-ui Dialog, for the reason set out at
  length in CloudAuthModal: bits-ui unmounts a dialog from inside a
  `requestAnimationFrame` that waits on the panel's animations to settle, and a window
  that is not painting frames never gets there. The closed dialog then stays in the DOM
  with `pointer-events: none` still on `<body>`, so the app looks fine and ignores every
  click until a reload. A sync from another machine is enough to land a close in that
  window. `{#if}` tears the panel down synchronously, painted or not.

  For the same reason there is no entry animation: one that starts at `opacity: 0` and
  waits on frames would leave the panel invisible for as long as the window is hidden.

  Both halves are portalled to `<body>` because these are mounted deep inside the queue
  and the editor, where an ancestor with a transform or a filter would otherwise make
  `position: fixed` resolve against that ancestor instead of the viewport.
-->
{#if open}
    <div
        use:portal={"body"}
        class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm"
        role="presentation"
        onclick={handleCancel}
    ></div>

    <div
        use:portal={"body"}
        use:trapFocus
        class="fixed left-[50%] top-[50%] z-[100] w-full max-w-sm translate-x-[-50%] translate-y-[-50%] outline-none"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabindex="-1"
    >
        <div
            class="bg-popover text-popover-foreground border-border border shadow-lg rounded-lg p-6 space-y-4"
        >
            <div class="space-y-2">
                <h2
                    id={titleId}
                    class="text-lg font-semibold block tracking-tight"
                >
                    {title}
                </h2>
                <p id={descriptionId} class="text-sm text-muted-foreground">
                    {description}
                </p>
            </div>

            <div class="flex justify-end gap-2">
                <button
                    type="button"
                    class="px-3 py-2 text-sm font-medium hover:bg-muted rounded-md transition-colors"
                    onclick={handleCancel}
                >
                    {finalCancelText}
                </button>
                <button
                    bind:this={confirmBtn}
                    type="button"
                    class="{variant === 'destructive'
                        ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
                        : 'bg-primary text-primary-foreground hover:bg-primary/90'} px-3 py-2 text-sm font-medium rounded-md transition-colors"
                    onclick={handleConfirm}
                >
                    {finalConfirmText}
                </button>
            </div>
        </div>
    </div>
{/if}
