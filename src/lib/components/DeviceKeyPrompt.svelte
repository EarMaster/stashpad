<!--
SPDX-License-Identifier: AGPL-3.0-only

Copyright (C) 2026 Nico Wiedemann

This file is part of Stashpad.
Stashpad is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License, version 3,
as published by the Free Software Foundation.
This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
See the GNU Affero General Public License for more details.
-->

<!--
    Only ever shown on a machine with no OS credential store - no Windows Credential
    Manager, no macOS Keychain, no Secret Service. Everywhere else the status is
    "notNeeded" and this component never mounts.

    Two jobs, by status:
      unset  - offer the choice, once: set a passphrase, or work locally without one
      locked - ask for the passphrase that is already configured
-->
<script lang="ts">
    import { _ } from "$lib/i18n";
    import { Loader2, KeyRound, ShieldAlert } from "lucide-svelte";
    import type { LocalKeyStatus } from "$lib/types";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";

    interface Props {
        status: LocalKeyStatus;
        onResolved: () => void;
    }

    let { status, onResolved }: Props = $props();

    const adapter = new DesktopStorageAdapter();

    let passphrase = $state("");
    let confirmation = $state("");
    let remember = $state(false);
    let busy = $state(false);
    let error = $state("");
    let panel = $state<HTMLDivElement | null>(null);

    const isSetup = $derived(status === "unset");
    const canSubmit = $derived(
        passphrase.trim().length > 0 &&
            (!isSetup || passphrase === confirmation) &&
            !busy,
    );

    $effect(() => {
        panel?.focus();
    });

    async function submit(event?: Event) {
        event?.preventDefault();
        if (!canSubmit) return;

        busy = true;
        error = "";
        try {
            if (isSetup) {
                await adapter.setLocalPassphrase(passphrase, remember);
                onResolved();
            } else {
                const opened = await adapter.unlockLocalKey(passphrase);
                if (!opened) {
                    error = $_("deviceKey.wrongPassphrase");
                    return;
                }
                onResolved();
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
            passphrase = "";
            confirmation = "";
        }
    }

    async function declineKey() {
        busy = true;
        error = "";
        try {
            await adapter.declineLocalKey();
            onResolved();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<div
    class="fixed inset-0 z-[100] bg-black/60 backdrop-blur-sm"
    role="presentation"
></div>

<div
    bind:this={panel}
    class="fixed left-[50%] top-[50%] z-[100] w-full max-w-md translate-x-[-50%] translate-y-[-50%] outline-none px-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="device-key-title"
    tabindex="-1"
>
    <div
        class="bg-popover text-popover-foreground border border-border shadow-xl rounded-xl overflow-hidden"
    >
        <div class="flex items-center gap-2 px-5 pt-5 pb-3">
            <KeyRound size={18} class="text-primary" aria-hidden="true" />
            <h2 id="device-key-title" class="text-base font-semibold tracking-tight">
                {isSetup ? $_("deviceKey.setupTitle") : $_("deviceKey.unlockTitle")}
            </h2>
        </div>

        <form class="px-5 pb-5 space-y-4" onsubmit={submit}>
            {#if isSetup}
                <p class="text-sm text-muted-foreground">
                    {$_("deviceKey.noKeyStore")}
                </p>
            {/if}

            <div class="space-y-2">
                <label class="block text-xs font-medium" for="device-key-passphrase">
                    {$_("deviceKey.passphraseLabel")}
                </label>
                <!-- svelte-ignore a11y_autofocus -->
                <input
                    id="device-key-passphrase"
                    type="password"
                    autocomplete={isSetup ? "new-password" : "current-password"}
                    autofocus
                    bind:value={passphrase}
                    disabled={busy}
                    class="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-primary"
                />
            </div>

            {#if isSetup}
                <div class="space-y-2">
                    <label class="block text-xs font-medium" for="device-key-confirm">
                        {$_("deviceKey.confirmLabel")}
                    </label>
                    <input
                        id="device-key-confirm"
                        type="password"
                        autocomplete="new-password"
                        bind:value={confirmation}
                        disabled={busy}
                        class="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-primary"
                    />
                    {#if confirmation.length > 0 && passphrase !== confirmation}
                        <p class="text-xs text-destructive">
                            {$_("deviceKey.confirmMismatch")}
                        </p>
                    {/if}
                </div>

                <label
                    class="flex items-start gap-2 rounded-md border border-border/60 p-3 text-xs"
                >
                    <input
                        type="checkbox"
                        bind:checked={remember}
                        disabled={busy}
                        class="mt-0.5"
                    />
                    <span>
                        <span class="font-medium block">{$_("deviceKey.rememberLabel")}</span>
                        <span class="text-muted-foreground"
                            >{$_("deviceKey.rememberWarning")}</span
                        >
                    </span>
                </label>

                <p class="text-xs text-muted-foreground">
                    {$_("deviceKey.forgotNote")}
                </p>
            {/if}

            {#if error}
                <p
                    class="flex items-start gap-1.5 text-xs text-destructive"
                    role="alert"
                >
                    <ShieldAlert size={14} class="mt-px shrink-0" aria-hidden="true" />
                    {error}
                </p>
            {/if}

            <div class="flex items-center justify-end gap-2 pt-1">
                {#if isSetup}
                    <button
                        type="button"
                        class="text-xs text-muted-foreground hover:text-foreground disabled:opacity-50"
                        disabled={busy}
                        onclick={declineKey}
                    >
                        {$_("deviceKey.workLocally")}
                    </button>
                {/if}
                <button
                    type="submit"
                    class="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground disabled:opacity-50"
                    disabled={!canSubmit}
                >
                    {#if busy}
                        <Loader2 size={14} class="animate-spin" aria-hidden="true" />
                    {/if}
                    {isSetup ? $_("deviceKey.setupAction") : $_("deviceKey.unlockAction")}
                </button>
            </div>
        </form>
    </div>
</div>
