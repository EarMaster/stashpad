<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { fade } from "svelte/transition";
    import { openUrl } from "@tauri-apps/plugin-opener";
    import { websiteOriginFromEndpoint } from "$lib/utils/cloud-urls";
    import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import { getOrCreateDeviceId } from "$lib/services/cloud-sync";
    import {
        Loader2,
        ExternalLink,
        ChevronDown,
        ChevronUp,
        Check,
    } from "lucide-svelte";
    import type { Settings } from "$lib/types";

    let {
        open = $bindable(false),
        settings = $bindable(),
        onSuccess,
        onCancel,
    } = $props<{
        open: boolean;
        settings: Settings;
        onSuccess: () => void;
        onCancel: () => void;
    }>();

    /**
     * Current phase of the authorization flow.
     * - waiting: browser opened, waiting for user action
     * - success: token exchanged successfully
     */
    type AuthPhase = "waiting" | "success";

    let phase = $state<AuthPhase>("waiting");

    /** Whether the manual code entry panel is expanded */
    let showManualEntry = $state(false);

    /** The raw link code pasted by the user */
    let linkCode = $state("");

    /** Error message from a failed exchange attempt */
    let linkCodeError = $state<string | null>(null);

    /** Whether a code exchange request is in flight */
    let linkCodeLoading = $state(false);

    /** The modal panel, focused on open so Escape and Tab start from inside it. */
    let panel = $state<HTMLDivElement | null>(null);

    /** Escape dismisses the modal, matching every other dialog in the app. */
    function handleKeydown(event: KeyboardEvent): void {
        if (open && event.key === "Escape") {
            event.preventDefault();
            onCancel();
        }
    }

    /**
     * Derives the frontend account URL from the configured cloud API endpoint.
     * Falls back to the API endpoint's /account/home route if the endpoint does
     * not follow the expected pattern.
     */
    function getAccountUrl(): string {
        const endpoint = settings.cloudConfig?.endpoint;
        if (!endpoint) return "";

        const origin = websiteOriginFromEndpoint(endpoint);
        return origin
            ? `${origin}/account?action=link-desktop`
            : `${endpoint}/account/home?action=link-desktop`;
    }

    /**
     * Opens the authorization URL in the system browser.
     *
     * Note that this hands the foreground to another application, so from here until
     * the user comes back the Stashpad window may not be painting frames at all. The
     * markup below does not depend on any that never arrive.
     */
    function openBrowser(): void {
        openUrl(getAccountUrl()).catch((err) => {
            console.error("[CloudAuthModal] Failed to open browser:", err);
        });
    }

    /**
     * Sanitizes a raw error string so that HTML responses (e.g. a server
     * 500 page) are replaced with a concise, readable message.
     */
    function sanitizeError(raw: string): string {
        const trimmed = raw.trim();
        // Detect HTML: starts with '<' or contains common HTML tags
        if (trimmed.startsWith("<") || /<html|<body|<!DOCTYPE/i.test(trimmed)) {
            return "Server error. Please try again or enter the code manually.";
        }
        // Truncate excessively long messages
        if (trimmed.length > 200) {
            return trimmed.slice(0, 200) + "…";
        }
        return trimmed || "Failed to link account";
    }

    /**
     * Exchanges the manually entered link code for an access token.
     * On success, saves the token to settings and emits `onSuccess`.
     */
    async function exchangeLinkCode(): Promise<void> {
        if (!linkCode.trim()) return;

        linkCodeLoading = true;
        linkCodeError = null;

        try {
            const adapter = new DesktopStorageAdapter();
            const data = await adapter.exchangeLinkCodeApi(
                linkCode.trim(),
                await getOrCreateDeviceId(adapter),
            );

            // Persist the new status in settings state
            if (settings.cloudConfig) {
                settings.cloudConfig.userId = data.userId;
                settings.cloudConfig.enabled = true;
            }

            phase = "success";

            // Give the user a moment to see the success state before closing
            setTimeout(() => {
                onSuccess();
            }, 1200);
        } catch (e) {
            const raw = e instanceof Error ? e.message : String(e);
            linkCodeError = sanitizeError(raw);
            // Expand the manual entry panel so the user can retry
            showManualEntry = true;
        } finally {
            linkCodeLoading = false;
        }
    }

    // Listen for deep-link events – only while the modal is open
    $effect(() => {
        if (!open) return;

        let unlisten: (() => void) | undefined;
        // Registration is async, so the modal can be dismissed before it finishes. Without
        // this the teardown would find `unlisten` still undefined and the listener would
        // outlive the modal, stacking one more handler on every open.
        let cancelled = false;

        const setupListener = async () => {
            const stop = await onOpenUrl((urls) => {
                for (const url of urls) {
                    if (url.startsWith("stashpad://auth/callback")) {
                        try {
                            const parsedUrl = new URL(url);
                            const token = parsedUrl.searchParams.get("token");
                            if (token) {
                                linkCode = token;
                                exchangeLinkCode();
                            }
                        } catch (err) {
                            console.error(
                                "[CloudAuthModal] Failed to parse deep link URL:",
                                err,
                            );
                        }
                    }
                }
            });

            if (cancelled) {
                stop();
            } else {
                unlisten = stop;
            }
        };

        setupListener();

        return () => {
            cancelled = true;
            if (unlisten) unlisten();
        };
    });

    // Open the browser and reset state when the dialog is opened
    $effect(() => {
        if (open) {
            phase = "waiting";
            showManualEntry = false;
            linkCode = "";
            linkCodeError = null;
            linkCodeLoading = false;
            openBrowser();
        }
    });

    // Move focus into the panel once it exists, so Escape and Tab act on the modal
    // rather than on whatever was focused in the settings page behind it.
    $effect(() => {
        panel?.focus();
    });
</script>

<svelte:window onkeydown={handleKeydown} />

<!--
  A plain `{#if}` overlay rather than the bits-ui Dialog the other modals use.
  See the note on `openBrowser` above: this is the one modal that sends the window
  to the background the moment it appears, and bits-ui only unmounts a dialog from
  inside a `requestAnimationFrame` that waits on the panel's animations to settle.
  A window that is not painting frames never gets there, so the closed dialog stayed
  in the DOM with `pointer-events: none` still on `<body>` and an invisible backdrop
  over everything - the app looked fine and ignored every click until a reload.
  `{#if}` tears the panel down synchronously, whether or not a frame is ever drawn.

  For the same reason this modal appears without the fade the others use: an entry
  animation that starts at `opacity: 0` and then waits on frames would leave the panel
  invisible for as long as the window is not painting.

  `position: fixed` is enough to escape the settings pane; no ancestor establishes a
  containing block for it (no transform, filter, or perspective on the way up).
-->
{#if open}
    <!-- Backdrop -->
    <div
        class="fixed inset-0 z-[100] bg-black/60 backdrop-blur-sm"
        role="presentation"
        onclick={onCancel}
    ></div>

    <!-- Modal panel -->
    <div
        bind:this={panel}
        class="fixed left-[50%] top-[50%] z-[100] w-full max-w-sm translate-x-[-50%] translate-y-[-50%] outline-none px-4"
        role="dialog"
        aria-modal="true"
        aria-labelledby="cloud-auth-title"
        tabindex="-1"
    >
        <div
            class="bg-popover text-popover-foreground border border-border shadow-xl rounded-xl overflow-hidden"
        >
                <!-- Header -->
                <div class="flex items-center justify-between px-5 pt-5 pb-3">
                    <h2
                        id="cloud-auth-title"
                        class="text-base font-semibold tracking-tight block"
                    >
                        {$_("settings.cloudSync.auth.modalTitle")}
                    </h2>
                </div>

                <div class="px-5 pb-5 space-y-4">
                    {#if phase === "waiting"}
                        <!-- Spinner + waiting message -->
                        <div class="flex flex-col items-center gap-3 py-4">
                            <Loader2
                                size={32}
                                class="animate-spin text-primary"
                                aria-hidden="true"
                            />
                            <p
                                class="text-sm text-center text-muted-foreground whitespace-pre-line"
                            >
                                {$_("settings.cloudSync.auth.waitingForAuth")}
                            </p>
                        </div>

                        <!-- Fallback link if browser didn't open -->
                        <div class="text-center">
                            <button
                                type="button"
                                class="inline-flex items-center gap-1.5 text-xs text-primary hover:underline"
                                onclick={openBrowser}
                            >
                                <ExternalLink size={12} aria-hidden="true" />
                                {$_(
                                    "settings.cloudSync.auth.openBrowserFallback",
                                )}
                            </button>
                        </div>

                        <hr class="border-border" />

                        <!-- Error banner – always visible when an exchange fails -->
                        {#if linkCodeError}
                            <div
                                class="flex items-start gap-2 rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2"
                                role="alert"
                                transition:fade={{ duration: 100 }}
                            >
                                <span class="shrink-0 text-red-500 text-base leading-none mt-px">⚠</span>
                                <p class="text-xs text-red-500 leading-snug">{linkCodeError}</p>
                            </div>
                        {/if}

                        <!-- Manual code entry disclosure -->
                        <div class="space-y-2">
                            <button
                                type="button"
                                class="w-full flex items-center justify-between text-xs text-muted-foreground hover:text-foreground transition-colors"
                                onclick={() => {
                                    showManualEntry = !showManualEntry;
                                }}
                                aria-expanded={showManualEntry}
                            >
                                <span
                                    >{$_(
                                        "settings.cloudSync.auth.enterCodeManually",
                                    )}</span
                                >
                                {#if showManualEntry}
                                    <ChevronUp size={14} aria-hidden="true" />
                                {:else}
                                    <ChevronDown size={14} aria-hidden="true" />
                                {/if}
                            </button>

                            {#if showManualEntry}
                                <div
                                    class="space-y-2"
                                    transition:fade={{ duration: 100 }}
                                >
                                    <div class="flex items-center gap-2">
                                        <input
                                            type="text"
                                            id="cloud-link-code"
                                            class="flex-1 rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors font-mono"
                                            placeholder={$_(
                                                "settings.cloudSync.auth.linkCodePlaceholder",
                                            )}
                                            bind:value={linkCode}
                                            onkeydown={(e) => {
                                                if (e.key === "Enter")
                                                    exchangeLinkCode();
                                            }}
                                            autocomplete="off"
                                            spellcheck={false}
                                        />
                                        <button
                                            type="button"
                                            class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
                                            onclick={exchangeLinkCode}
                                            disabled={linkCodeLoading ||
                                                !linkCode.trim()}
                                        >
                                            {linkCodeLoading
                                                ? $_(
                                                      "settings.cloudSync.auth.linking",
                                                  )
                                                : $_(
                                                      "settings.cloudSync.auth.link",
                                                  )}
                                        </button>
                                    </div>
                                </div>
                            {/if}
                        </div>

                        <!-- Cancel button row -->
                        <div class="pt-1 flex justify-end">
                            <button
                                type="button"
                                class="px-4 py-1.5 rounded-md text-sm border border-border hover:bg-muted transition-colors"
                                onclick={onCancel}
                            >
                                {$_("common.cancel")}
                            </button>
                        </div>
                    {:else}
                        <!-- Success state -->
                        <div class="flex flex-col items-center gap-3 py-6">
                            <div
                                class="w-12 h-12 rounded-full bg-green-500/10 flex items-center justify-center"
                            >
                                <Check
                                    size={24}
                                    class="text-green-500"
                                    aria-hidden="true"
                                />
                            </div>
                            <p class="text-sm font-medium text-center">
                                {$_("settings.cloudSync.auth.authSuccess")}
                            </p>
                        </div>
                    {/if}
                </div>
        </div>
    </div>
{/if}
