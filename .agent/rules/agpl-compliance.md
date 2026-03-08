---
trigger: always_on
---

# LICENSE HEADER (AGPLv3 Compliance)

## 1. Objective
Ensure that the copyleft effect of the AGPL v3 is maintained by adding the necessary license header to every source code file for the Stashpad app.

DO NOT ADD THESE TO cloud OR website FILES!

## 2. Affected Files
Must be applied to the top of EVERY new and existing source file:
- Rust files (`.rs`)
- TypeScript files (`.ts`)
- Svelte files (`.svelte`)

## 3. Header Content
Insert the following block into the header of the respective file. Adapt the commenting style (`//` or ``) to the target language.

**IMPORTANT:** Substitute the placeholders `[CURRENT_YEAR]` and `[AUTHOR_NAME]` with the correct values for the project copyright.

```
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) [CURRENT\_YEAR] [AUTHOR\_NAME]
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

```