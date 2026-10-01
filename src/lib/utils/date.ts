
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
import { localTimeZone, now, parseInstant } from "./time";

export function getRelativeTime(dateString: string, translate: (key: string, values?: any) => string) {
    const instant = parseInstant(dateString);
    if (!instant) return "";
    const current = now();
    const diffInSeconds = Math.floor(
        (current.epochMilliseconds - instant.epochMilliseconds) / 1000,
    );

    if (diffInSeconds < 60) {
        return translate("contextSwitcher.time.justNow");
    }

    const diffInMinutes = Math.floor(diffInSeconds / 60);
    if (diffInMinutes < 60) {
        return diffInMinutes === 1
            ? translate("contextSwitcher.time.minute", {
                values: { count: diffInMinutes },
            })
            : translate("contextSwitcher.time.minutes", {
                values: { count: diffInMinutes },
            });
    }

    const diffInHours = Math.floor(diffInMinutes / 60);
    if (diffInHours < 24) {
        return diffInHours === 1
            ? translate("contextSwitcher.time.hour", {
                values: { count: diffInHours },
            })
            : translate("contextSwitcher.time.hours", {
                values: { count: diffInHours },
            });
    }

    const diffInDays = Math.floor(diffInHours / 24);
    if (diffInDays === 1) {
        return translate("contextSwitcher.time.yesterday");
    }
    if (diffInDays < 7) {
        return translate("contextSwitcher.time.daysAgo", {
            values: { count: diffInDays },
        });
    }

    // Check if same year, on the computer's own calendar
    const timeZone = localTimeZone();
    const sameYear =
        instant.toZonedDateTimeISO(timeZone).year === current.toZonedDateTimeISO(timeZone).year;
    if (sameYear) {
        return instant.toLocaleString(undefined, {
            month: "short",
            day: "numeric",
            timeZone,
        });
    }

    return instant.toLocaleString(undefined, {
        month: "short",
        year: "numeric",
        timeZone,
    });
}
