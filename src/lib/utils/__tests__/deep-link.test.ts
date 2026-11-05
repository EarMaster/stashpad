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
import { describe, it, expect, vi, beforeEach } from "vitest";

const plugin = vi.hoisted(() => ({
    onOpenUrl: vi.fn(),
    getCurrent: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-deep-link", () => plugin);

import { parseStashLink, onStashLink } from "../deep-link";

const ID = "dbbc1f3c-bc08-4c0d-86d7-acb5c935542a";

describe("parseStashLink", () => {
    it("reads the id from a stash link", () => {
        expect(parseStashLink(`stashpad://stash/${ID}`)).toBe(ID);
        expect(parseStashLink(`stashpad://stash/${ID}/`)).toBe(ID);
        expect(parseStashLink(`stashpad://stash/${ID.toUpperCase()}`)).toBe(ID);
    });

    it("ignores every other link", () => {
        expect(parseStashLink("stashpad://auth/callback?token=abc")).toBeNull();
        expect(parseStashLink("stashpad://stash/not-an-id")).toBeNull();
        expect(parseStashLink(`stashpad://stash/${ID}?x=1`)).toBeNull();
        expect(parseStashLink(`https://stashpad.org/stash/${ID}`)).toBeNull();
    });
});

describe("onStashLink", () => {
    beforeEach(() => {
        plugin.onOpenUrl.mockReset();
        plugin.getCurrent.mockReset();
    });

    it("hands over the launch link and later ones, and nothing else", async () => {
        let later: (urls: string[]) => void = () => {};
        const stop = vi.fn();
        plugin.onOpenUrl.mockImplementation(async (handler) => {
            later = handler;
            return stop;
        });
        plugin.getCurrent.mockResolvedValue([`stashpad://stash/${ID}`]);

        const seen: string[] = [];
        const unlisten = await onStashLink((id) => seen.push(id));
        expect(seen).toEqual([ID]);

        later(["stashpad://auth/callback?token=x", `stashpad://stash/${ID}`]);
        expect(seen).toEqual([ID, ID]);

        unlisten();
        expect(stop).toHaveBeenCalled();
    });

    it("still listens when the launch link cannot be read", async () => {
        plugin.onOpenUrl.mockResolvedValue(() => {});
        plugin.getCurrent.mockRejectedValue(new Error("no"));
        await expect(onStashLink(() => {})).resolves.toBeTypeOf("function");
    });
});
