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
    import { onMount, onDestroy } from "svelte";
    import { _ } from "$lib/i18n";
    import { Loader2, ShieldCheck, ShieldAlert, KeyRound, Copy } from "lucide-svelte";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { E2eeStatus, LocalKeyStatus } from "$lib/types";
    import DeviceKeyPrompt from "./DeviceKeyPrompt.svelte";
    import { errorText } from "$lib/errors";

    const adapter = new DesktopStorageAdapter();

    /// What the panel is polling for, when it is polling at all.
    type Waiting = "progress" | "approval" | null;

    let status = $state<E2eeStatus | null>(null);
    let busy = $state(false);
    let error = $state("");

    // Shown once, held only in memory. There is no way to retrieve it afterwards, which is
    // the point and has to be said on screen rather than discovered.
    let freshCode = $state("");
    let typeBack = $state("");
    // The sweep is carried by ordinary syncs, so nothing tells this panel when it moves.
    // It used to hold the one-off return of e2eeStartConversion, which meant the figure
    // froze at whatever the corpus was when conversion began and read 0 after a reload.
    // Now it is polled while the account is migrating.
    let remaining = $state(0);
    let total = $state(0);

    // Where this installation's key is kept. Worth stating rather than leaving to be
    // inferred from whether a passphrase dialog appeared: a machine that wrongly decides
    // it has no credential store looks identical to one that genuinely has none, and the
    // only visible difference was a prompt appearing at some earlier point.
    let keyStore = $state<LocalKeyStatus | null>(null);

    /// Whether the passphrase dialog is open, asked for from this panel.
    let askForPassphrase = $state(false);

    /// What to open the dialog as.
    ///
    /// It only understands `unset` (choose one) and `locked` (type the one you have), so
    /// `declined` maps to `unset`: someone who waved it away and wants it after all is
    /// setting one for the first time.
    const passphraseMode = $derived<"unset" | "locked">(
        keyStore === "locked" ? "locked" : "unset",
    );
    let pollTimer: ReturnType<typeof setInterval> | null = null;
    let pollingFor = $state<Waiting>(null);

    /// The progress count is a local database query, so it can be asked for often. The
    /// approval check is a request to the server, so it is not.
    const PROGRESS_POLL_MS = 2000;
    const APPROVAL_POLL_MS = 4000;

    /// Records already through, for the bar. Clamped because `total` is counted fresh and
    /// a record deleted mid-sweep can otherwise make this exceed `total`.
    const done = $derived(Math.max(0, Math.min(total, total - remaining)));
    const percent = $derived(total > 0 ? Math.round((done / total) * 100) : 0);

    /// The installation whose codes the user has ticked off as matching.
    ///
    /// One at a time: approving is a deliberate act per machine, and a tick that survived
    /// from the previous card would be the wrong kind of convenience.
    let confirmedDevice = $state<string | null>(null);
    let recoveryInput = $state("");

    const pending = $derived(status?.devices.filter((d) => d.status === "pending") ?? []);
    const active = $derived(status?.devices.filter((d) => d.status === "active") ?? []);

    /// The last group of the code, which is what the type-back asks for. Enough to show the
    /// user actually has it in front of them without making them retype fifty characters.
    const expectedGroup = $derived(freshCode.split("-").pop() ?? "");

    onMount(refresh);

    onDestroy(stopPolling);

    /// What the panel is waiting on, if anything.
    ///
    /// Both of these move somewhere the panel cannot see. The sweep is carried by ordinary
    /// syncs, and approval happens on another machine entirely - so nothing tells this
    /// panel when either has happened and it has to look. An installation that had just
    /// been let in used to go on saying it was waiting until Settings was closed and
    /// reopened, which reads as the approval not having worked.
    /// Waiting to be let in comes first. An installation that cannot open the key can do
    /// nothing about the sweep either, so polling its progress would watch the one number
    /// that cannot change for it while missing the one that can.
    function waitingOn(next: E2eeStatus | null): Waiting {
        if (!next || next.state === "off") return null;
        if (!next.unlocked) return "approval";
        if (next.state === "migrating") return "progress";
        return null;
    }

    /// Poll only while there is something to watch, and stop as soon as there is not: an
    /// interval left running behind a closed settings page would keep asking for the life
    /// of the process. Re-entering with the same answer leaves the existing timer alone,
    /// so a poll that refreshes cannot reset its own interval.
    function syncPolling(next: Waiting) {
        if (next === pollingFor) return;
        stopPolling();
        pollingFor = next;
        if (next === "progress") {
            pollTimer = setInterval(readProgress, PROGRESS_POLL_MS);
        } else if (next === "approval") {
            // e2ee_status opens this installation's wrap if the server is holding one, so
            // asking is also the act of picking the key up the moment it is granted.
            pollTimer = setInterval(() => void refresh({ quiet: true }), APPROVAL_POLL_MS);
        }
    }

    function stopPolling() {
        if (pollTimer) {
            clearInterval(pollTimer);
            pollTimer = null;
        }
        pollingFor = null;
    }

    async function readProgress() {
        try {
            const progress = await adapter.e2eeConversionProgress();
            remaining = progress.remaining;
            total = progress.total;
            // The sweep finishing is not something the sync layer announces, so the only
            // way the panel learns the account is sealable is by looking again.
            if (remaining === 0) {
                stopPolling();
            }
        } catch {
            // A failed poll is not worth a banner over the panel; the next one will tell
            // the same story, and the count simply holds its last value meanwhile.
        }
    }

    /// Read the whole panel back.
    ///
    /// `quiet` is for the poll, and must touch neither `busy` nor `error`: flipping `busy`
    /// every few seconds would grey out the buttons under the person's cursor, and
    /// clearing `error` would wipe a message they are still reading. A failed poll says
    /// nothing the next one will not say again.
    async function refresh(options: { quiet?: boolean } = {}) {
        const quiet = options.quiet === true;
        // Sets `busy` itself so the retry button below can disable while it runs; `run()`
        // calls this too, and setting the flag twice is harmless.
        if (!quiet) busy = true;
        try {
            const next = await adapter.e2eeStatus();
            status = next;
            if (!quiet) error = "";
            try {
                keyStore = await adapter.localKeyStatus();
            } catch {
                // Not worth failing the panel over; the line simply does not render.
                keyStore = null;
            }
            // Read the count straight away rather than waiting a poll interval, so
            // reopening the page mid-sweep shows the real figure instead of a zero.
            if (next.state === "migrating") {
                await readProgress();
            }
            syncPolling(waitingOn(next));
        } catch (e) {
            if (!quiet) error = errorText(e);
        } finally {
            if (!quiet) busy = false;
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
            await adapter.e2eeStartConversion();
            freshCode = "";
            typeBack = "";
            // `run` calls refresh() next, which reads the real count and starts the poll.
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

    <!-- Where the key lives. Shown in every state, including before encryption is turned
         on, because it is also the answer to "why was I asked for a passphrase" - and on a
         machine that has a keychain but failed to use it, this is the only place that
         difference is visible at all. -->
    {#if keyStore}
        <p class="flex items-start gap-1.5 text-xs text-muted-foreground">
            {#if keyStore === "notNeeded"}
                <ShieldCheck size={14} class="mt-px shrink-0 text-primary" aria-hidden="true" />
                {$_("encryption.keptInKeychain")}
            {:else if keyStore === "unlocked" || keyStore === "locked"}
                <KeyRound size={14} class="mt-px shrink-0" aria-hidden="true" />
                {$_("encryption.keptUnderPassphrase")}
            {:else}
                <ShieldAlert size={14} class="mt-px shrink-0 text-red-500" aria-hidden="true" />
                {$_("encryption.keptNowhere")}
            {/if}
        </p>

        <!-- A way to actually set one.
             The error above tells people to set a device passphrase, and until now there
             was nowhere to do it: the dialog is rendered from App.svelte only while the
             status is `unset` or `locked`, so anyone who chose "work locally without one"
             moved to `declined` and could never get back to it. Offered here whenever a
             passphrase would help, which on a machine with no usable credential store is
             the only thing standing between the app and being unable to keep a secret. -->
        {#if keyStore === "unset" || keyStore === "locked" || keyStore === "declined"}
            <button
                type="button"
                onclick={() => (askForPassphrase = true)}
                class="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
            >
                <KeyRound size={12} aria-hidden="true" />
                {keyStore === "locked"
                    ? $_("encryption.unlockPassphrase")
                    : $_("encryption.setPassphrase")}
            </button>
        {/if}
    {/if}

    {#if error}
        <p class="flex items-start gap-1.5 text-xs text-red-500" role="alert">
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
                onclick={() => refresh()}
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

            <p class="text-xs font-medium text-red-500">
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
            <div class="space-y-2">
                <p class="text-xs text-muted-foreground">
                    {#if remaining > 0}
                        {$_("encryption.converting", { values: { count: remaining } })}
                    {:else}
                        {$_("encryption.convertingDone")}
                    {/if}
                </p>

                <!-- The sweep runs for minutes on a large account, so it needs to show
                     movement rather than only a number that changes every few seconds. -->
                <div
                    class="h-1.5 w-full overflow-hidden rounded-full bg-muted"
                    role="progressbar"
                    aria-valuenow={percent}
                    aria-valuemin="0"
                    aria-valuemax="100"
                    aria-label={$_("encryption.progressLabel")}
                >
                    <div
                        class="h-full rounded-full bg-primary transition-all duration-500"
                        style="width: {percent}%"
                    ></div>
                </div>

                <p class="text-xs text-muted-foreground tabular-nums">
                    {$_("encryption.progressCount", {
                        values: { done, total, percent },
                    })}
                </p>
            </div>

            <!-- Was an outline button, which on this card read as disabled for the whole
                 sweep - the one thing it must not look like, since it is the action that
                 finishes encryption. It is the primary action here, so it looks like one,
                 and it is genuinely disabled only while records are still outstanding. -->
            <button
                type="button"
                class="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={busy || remaining > 0}
                title={remaining > 0 ? $_("encryption.finishBlocked") : undefined}
                onclick={() => run(() => adapter.e2eeSeal())}
            >
                {#if busy}<Loader2 size={12} class="animate-spin" aria-hidden="true" />{/if}
                {$_("encryption.finish")}
            </button>
        {:else}
            <p class="text-xs text-muted-foreground">{$_("encryption.sealedExplainer")}</p>
        {/if}

        {#if !status.unlocked}
            <div class="space-y-2 rounded-lg border border-border bg-card p-3">
                <p class="text-xs">{$_("encryption.lockedHere")}</p>
                <!-- Only when there is actually another installation to approve from.
                     Adding a machine usually happens *because* the other one is not to
                     hand - a new work laptop, or one that broke - so offering approval as
                     the headline was advice for the rarer case, and unusable advice for
                     the common one. The recovery code leads; this appears only when it is
                     something the person can really do. -->
                {#if active.length > 0}
                    <p class="text-xs text-muted-foreground">
                        {$_("encryption.lockedApproveAlternative")}
                    </p>
                {/if}
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

        <!--
            Installations waiting to be let in.

            The code used to be typed back into a field before the button appeared, which
            was two steps and a keyboard for something the eye has already done. A tick is
            the same assertion: the person either compared the two screens or decided not
            to, and retyping sixteen characters does not make the first more likely. What
            it did make more likely was a typo being read as a mismatch.

            The fingerprint still travels to the backend, which recomputes it from the key
            it fetches at that moment - so a key swapped between this card being drawn and
            the button being pressed is still caught. That check is not the typing; it
            never was.
        -->
        {#if pending.length > 0 && status.unlocked}
            <div class="space-y-2">
                <p class="flex items-center gap-1.5 text-xs font-medium">
                    <ShieldAlert size={14} class="shrink-0 text-primary" aria-hidden="true" />
                    {$_("encryption.pendingTitle")}
                </p>
                {#each pending as device (device.deviceId)}
                    <div class="space-y-3 rounded-lg border border-border bg-card p-3">
                        <p class="text-xs text-muted-foreground">
                            {$_("encryption.compareInstruction")}
                        </p>

                        <code
                            class="block select-all rounded bg-background p-2 text-center font-mono text-sm tracking-widest"
                            >{device.fingerprint}</code
                        >

                        <label
                            class="flex items-start gap-2 rounded-md border border-border/60 p-3 text-xs"
                        >
                            <input
                                type="checkbox"
                                checked={confirmedDevice === device.deviceId}
                                disabled={busy}
                                onchange={(e) =>
                                    (confirmedDevice = e.currentTarget.checked
                                        ? device.deviceId
                                        : null)}
                                class="mt-0.5"
                            />
                            <span>
                                <span class="block font-medium"
                                    >{$_("encryption.confirmMatch")}</span
                                >
                                <span class="text-muted-foreground"
                                    >{$_("encryption.confirmMatchHint")}</span
                                >
                            </span>
                        </label>

                        <button
                            type="button"
                            class="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
                            disabled={busy || confirmedDevice !== device.deviceId}
                            onclick={() =>
                                run(async () => {
                                    await adapter.e2eeApproveDevice(
                                        device.deviceId,
                                        device.fingerprint,
                                    );
                                    confirmedDevice = null;
                                })}
                        >
                            {#if busy}<Loader2
                                    size={12}
                                    class="animate-spin"
                                    aria-hidden="true"
                                />{/if}
                            {$_("encryption.approve")}
                        </button>
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

{#if askForPassphrase}
    <DeviceKeyPrompt
        status={passphraseMode}
        onResolved={() => {
            askForPassphrase = false;
            // Re-reads keyStore and the encryption status: setting a passphrase is what
            // makes sealing possible, so the whole panel can change shape.
            void refresh();
        }}
    />
{/if}
