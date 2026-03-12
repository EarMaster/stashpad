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
    import { _ } from "$lib/i18n";
    import { Dialog } from "bits-ui";
    import { fade } from "svelte/transition";
    import { openUrl } from "@tauri-apps/plugin-opener";
    import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
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

    /**
     * Derives the web portal URL from the configured cloud endpoint.
     * Opens the account page so the website knows to show the linking flow.
     */
    function getAccountUrl(): string {
        const endpoint = settings.cloudConfig?.endpoint;
        if (!endpoint) return "";
        const baseUrl = endpoint.replace("https://api.", "https://");
        return `${baseUrl}/account?action=link-desktop`;
    }

    /** Opens the authorization URL in the system browser. */
    function openBrowser(): void {
        openUrl(getAccountUrl()).catch((err) => {
            console.error("[CloudAuthModal] Failed to open browser:", err);
        });
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
            const data = await new DesktopStorageAdapter().exchangeLinkCodeApi(
                linkCode.trim(),
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
            linkCodeError = e instanceof Error ? e.message : String(e) || "Failed to link account";
            showManualEntry = true;
        } finally {
            linkCodeLoading = false;
        }
    }

    // Listen for deep link events
    $effect(() => {
        let unlisten: (() => void) | undefined;

        const setupListener = async () => {
            unlisten = await onOpenUrl((urls) => {
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
        };

        setupListener();

        return () => {
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
</script>

<!--
  Uses bits-ui Dialog.Portal to teleport the modal to document.body,
  ensuring it escapes any parent overflow/transform constraints.
-->
<Dialog.Root
    bind:open
    onOpenChange={(v) => {
        if (!v) onCancel();
    }}
>
    <Dialog.Portal>
        <!-- Backdrop -->
        <Dialog.Overlay
            class="fixed inset-0 z-[100] bg-black/60 backdrop-blur-sm animate-in fade-in-0 duration-150"
        />

        <!-- Modal panel -->
        <Dialog.Content
            class="fixed left-[50%] top-[50%] z-[100] w-full max-w-sm translate-x-[-50%] translate-y-[-50%] outline-none px-4 animate-in zoom-in-95 fade-in-0 duration-200"
        >
            <div
                class="bg-popover text-popover-foreground border border-border shadow-xl rounded-xl overflow-hidden"
            >
                <!-- Header -->
                <div class="flex items-center justify-between px-5 pt-5 pb-3">
                    <Dialog.Title
                        class="text-base font-semibold tracking-tight block"
                    >
                        {$_("settings.cloudSync.auth.modalTitle")}
                    </Dialog.Title>
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

                                    {#if linkCodeError}
                                        <p
                                            class="text-xs text-red-500"
                                            role="alert"
                                        >
                                            {linkCodeError}
                                        </p>
                                    {/if}
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
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>
