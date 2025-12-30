<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { Dialog } from "bits-ui";
    import { fade, scale } from "svelte/transition";
    import { quintOut } from "svelte/easing";

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

    let finalConfirmText = $derived(confirmText || $_("common.save"));
    let finalCancelText = $derived(cancelText || $_("common.cancel"));

    // Reset confirmed state when dialog opens
    $effect(() => {
        if (open) isConfirmed = false;
    });

    // Ensure parent state is synchronized when dialog is closed/dismissed
    $effect(() => {
        if (!open && !isConfirmed) {
            onCancel?.();
        }
    });

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

<Dialog.Root bind:open>
    <Dialog.Portal>
        <Dialog.Overlay
            transition={fade}
            transitionConfig={{ duration: 150 }}
            class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm"
        />
        <Dialog.Content
            class="fixed left-[50%] top-[50%] z-[100] w-full max-w-sm translate-x-[-50%] translate-y-[-50%] outline-none"
            transition={scale}
            transitionConfig={{
                duration: 200,
                start: 0.95,
                opacity: 0,
                easing: quintOut,
            }}
            onOpenAutoFocus={(e) => {
                e.preventDefault();
                confirmBtn?.focus();
            }}
        >
            <div
                class="bg-popover text-popover-foreground border-border border shadow-lg rounded-lg p-6 space-y-4"
            >
                <div class="space-y-2">
                    <Dialog.Title
                        class="text-lg font-semibold block tracking-tight"
                    >
                        {title}
                    </Dialog.Title>
                    <Dialog.Description class="text-sm text-muted-foreground">
                        {description}
                    </Dialog.Description>
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
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>
