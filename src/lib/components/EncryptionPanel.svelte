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
    Content encryption, under Settings › Cloud Sync.

    Four states, and the panel shows exactly one:
      off        - offer to turn it on, with what it costs stated before the button
      recovery   - the code, shown once, with a forced type-back before anything converts
      converting - progress, and the button that finishes
      sealed     - the installation list, and the way to let another machine in

    The fingerprints shown here are recomputed on this machine from each published key. The
    server's stored copy is never displayed: having the user compare two numbers the server
    supplied would verify nothing, and that comparison is the whole defence against a key
    being substituted at enrolment.
-->
<script lang="ts">
    import { onMount } from "svelte";
    import { _ } from "$lib/i18n";
    import { Loader2, ShieldCheck, ShieldAlert, KeyRound, Copy } from "lucide-svelte";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { E2eeStatus } from "$lib/types";
    import { errorText } from "$lib/errors";

    const adapter = new DesktopStorageAdapter();

    let status = $state<E2eeStatus | null>(null);
    let busy = $state(false);
    let error = $state("");

    // Shown once, held only in memory. There is no way to retrieve it afterwards, which is
    // the point and has to be said on screen rather than discovered.
    let freshCode = $state("");
    let typeBack = $state("");
    let converting = $state(0);

    let approving = $state<string | null>(null);
    let approvalFingerprint = $state("");
    let recoveryInput = $state("");

    const pending = $derived(status?.devices.filter((d) => d.status === "pending") ?? []);
    const active = $derived(status?.devices.filter((d) => d.status === "active") ?? []);

    /// The last group of the code, which is what the type-back asks for. Enough to show the
    /// user actually has it in front of them without making them retype fifty characters.
    const expectedGroup = $derived(freshCode.split("-").pop() ?? "");

    onMount(refresh);

    async function refresh() {
        // Sets `busy` itself so the retry button below can disable while it runs; `run()`
        // calls this too, and setting the flag twice is harmless.
        busy = true;
        try {
            status = await adapter.e2eeStatus();
            error = "";
        } catch (e) {
            error = errorText(e);
        } finally {
            busy = false;
        }
    }

    async function run(action: () => Promise<void>) {
        busy = true;
        error = "";
        try {
            await action();
            await refresh();
        } catch (e) {
            error = errorText(e);
        } finally {
            busy = false;
        }
    }

    async function enable() {
        await run(async () => {
            const result = await adapter.e2eeEnable();
            freshCode = result.recoveryCode;
        });
    }

    async function confirmWrittenDown() {
        if (typeBack.trim().toUpperCase() !== expectedGroup.toUpperCase()) {
            error = $_("encryption.typeBackMismatch");
            return;
        }
        await run(async () => {
            await adapter.e2eeAcknowledgeRecovery();
            converting = await adapter.e2eeStartConversion();
            freshCode = "";
            typeBack = "";
        });
    }

    async function copyCode() {
        try {
            await navigator.clipboard.writeText(freshCode);
        } catch {
            // Clipboard access can be refused; the code is on screen either way.
        }
    }
</script>

<div class="space-y-4">
    <h3 class="text-sm font-semibold flex items-center gap-2">
        {#if status?.state === "sealed"}
            <ShieldCheck size={16} class="text-primary" aria-hidden="true" />
        {:else}
            <KeyRound size={16} class="text-muted-foreground" aria-hidden="true" />
        {/if}
        {$_("encryption.title")}
    </h3>

    {#if error}
        <p class="flex items-start gap-1.5 text-xs text-destructive" role="alert">
            <ShieldAlert size={14} class="mt-px shrink-0" aria-hidden="true" />
            {error}
        </p>
    {/if}

    {#if !status}
        <!-- A failed first load used to leave this branch showing "checking" for good:
             `status` stays null when refresh() throws, so the panel reported that it was
             still working while the error sat above it, and offered no way to try again. -->
        {#if error}
            <button
                type="button"
                onclick={refresh}
                disabled={busy}
                class="text-xs underline text-muted-foreground hover:text-foreground disabled:opacity-50"
            >
                {$_("encryption.retry")}
            </button>
        {:else}
            <p class="text-xs text-muted-foreground">{$_("encryption.loading")}</p>
        {/if}

    <!-- The recovery code, shown once. Nothing converts until it is acknowledged. -->
    {:else if freshCode}
        <div class="space-y-3 rounded-lg border border-primary/40 bg-card p-3">
            <p class="text-xs text-muted-foreground">{$_("encryption.recoveryIntro")}</p>

            <div class="flex items-start gap-2">
                <code
                    class="flex-1 select-all break-all rounded bg-background p-2 font-mono text-xs leading-relaxed"
                    >{freshCode}</code
                >
                <button
                    type="button"
                    class="shrink-0 rounded border border-border p-1.5 hover:bg-muted"
                    onclick={copyCode}
                    aria-label={$_("encryption.copyCode")}
                >
                    <Copy size={14} aria-hidden="true" />
                </button>
            </div>

            <p class="text-xs font-medium text-destructive">
                {$_("encryption.recoveryWarning")}
            </p>

            <div class="space-y-1.5">
                <label class="block text-xs" for="encryption-typeback">
                    {$_("encryption.typeBackLabel", { values: { group: expectedGroup } })}
                </label>
                <input
                    id="encryption-typeback"
                    bind:value={typeBack}
                    disabled={busy}
                    class="w-32 rounded-md border border-border bg-background px-2 py-1 font-mono text-sm uppercase outline-none focus:ring-1 focus:ring-primary"
                />
            </div>

            <button
                type="button"
                class="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50"
                disabled={busy}
                onclick={confirmWrittenDown}
            >
                {#if busy}<Loader2 size={12} class="animate-spin" aria-hidden="true" />{/if}
                {$_("encryption.confirmWrittenDown")}
            </button>
        </div>

    <!-- Not yet encrypted. State the cost before the button, not after. -->
    {:else if status.state === "off"}
        <p class="text-xs text-muted-foreground">{$_("encryption.offExplainer")}</p>
        <ul class="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
            <li>{$_("encryption.costRecovery")}</li>
            <li>{$_("encryption.costMcp")}</li>
            <li>{$_("encryption.costLocal")}</li>
        </ul>
        <button
            type="button"
            class="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50"
            disabled={busy}
            onclick={enable}
        >
            {#if busy}<Loader2 size={12} class="animate-spin" aria-hidden="true" />{/if}
            {$_("encryption.turnOn")}
        </button>

    {:else}
        <!-- Converting or sealed. -->
        {#if status.state === "migrating"}
            <p class="text-xs text-muted-foreground">
                {$_("encryption.converting", { values: { count: converting } })}
            </p>
            <button
                type="button"
                class="rounded-md border border-border px-3 py-1.5 text-xs disabled:opacity-50"
                disabled={busy}
                onclick={() => run(() => adapter.e2eeSeal())}
            >
                {$_("encryption.finish")}
            </button>
        {:else}
            <p class="text-xs text-muted-foreground">{$_("encryption.sealedExplainer")}</p>
        {/if}

        {#if !status.unlocked}
            <div class="space-y-2 rounded-lg border border-border bg-card p-3">
                <p class="text-xs">{$_("encryption.lockedHere")}</p>
                <input
                    bind:value={recoveryInput}
                    placeholder="SP1-…"
                    disabled={busy}
                    class="w-full rounded-md border border-border bg-background px-2 py-1 font-mono text-xs outline-none focus:ring-1 focus:ring-primary"
                />
                <button
                    type="button"
                    class="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50"
                    disabled={busy || !recoveryInput.trim()}
                    onclick={() =>
                        run(async () => {
                            await adapter.e2eeRecover(recoveryInput);
                            recoveryInput = "";
                        })}
                >
                    {$_("encryption.useRecoveryCode")}
                </button>
            </div>
        {/if}

        <!-- Installations waiting to be let in. -->
        {#if pending.length > 0 && status.unlocked}
            <div class="space-y-2">
                <p class="text-xs font-medium">{$_("encryption.pendingTitle")}</p>
                {#each pending as device (device.deviceId)}
                    <div class="space-y-2 rounded-lg border border-border bg-card p-3">
                        <p class="text-xs text-muted-foreground">
                            {$_("encryption.compareInstruction")}
                        </p>
                        <code class="block font-mono text-sm">{device.fingerprint}</code>
                        {#if approving === device.deviceId}
                            <input
                                bind:value={approvalFingerprint}
                                placeholder="XXXX-XXXX-XXXX-XXXX"
                                class="w-full rounded-md border border-border bg-background px-2 py-1 font-mono text-xs uppercase outline-none focus:ring-1 focus:ring-primary"
                            />
                            <button
                                type="button"
                                class="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50"
                                disabled={busy}
                                onclick={() =>
                                    run(async () => {
                                        await adapter.e2eeApproveDevice(
                                            device.deviceId,
                                            approvalFingerprint,
                                        );
                                        approving = null;
                                        approvalFingerprint = "";
                                    })}
                            >
                                {$_("encryption.approve")}
                            </button>
                        {:else}
                            <button
                                type="button"
                                class="rounded-md border border-border px-3 py-1.5 text-xs"
                                onclick={() => (approving = device.deviceId)}
                            >
                                {$_("encryption.letItIn")}
                            </button>
                        {/if}
                    </div>
                {/each}
            </div>
        {/if}

        <div class="space-y-1">
            <p class="text-xs font-medium">{$_("encryption.thisInstallation")}</p>
            <code class="block font-mono text-xs text-muted-foreground"
                >{status.fingerprint}</code
            >
            <p class="text-xs text-muted-foreground">
                {$_("encryption.activeCount", { values: { count: active.length } })}
            </p>
        </div>
    {/if}
</div>
