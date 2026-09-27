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

import type { ContextRule } from "$lib/types";

/** One rule reduced to what it means, in a fixed shape. */
function canonical(rule: ContextRule): string {
    return JSON.stringify([
        rule.ruleType,
        rule.matchType,
        rule.value,
        rule.matchCase ?? false,
        rule.useRegex ?? false,
    ]);
}

/**
 * Whether two rule lists do the same thing.
 *
 * Comparing their JSON text, which the import dialog used to do, counted the order of each
 * rule's fields as a difference. An export writes them alphabetically - the Rust side goes
 * through `serde_json::Value`, whose map is sorted - while the app keeps them in the order
 * it builds them, so every context with rules came back as a conflict with itself. A
 * missing `matchCase` or `useRegex` is also the same as `false`, which is what both sides
 * treat it as. The order of the rules themselves is kept significant: it is the order they
 * are shown and edited in.
 */
export function sameRules(
    a: ContextRule[] | undefined,
    b: ContextRule[] | undefined,
): boolean {
    const left = (a ?? []).map(canonical);
    const right = (b ?? []).map(canonical);
    return left.length === right.length && left.every((rule, i) => rule === right[i]);
}
