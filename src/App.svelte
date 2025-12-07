<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
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
   import Header from "$lib/components/Header.svelte";
   import Editor from "$lib/components/Editor.svelte";
   import Queue from "$lib/components/Queue.svelte";

   let transferMode = $state("Drag");
   let refreshTrigger = $state(0);

   function handleStash() {
      refreshTrigger++;
   }
</script>

<main
   class="h-screen w-screen flex flex-col bg-background text-foreground overflow-hidden font-sans select-none"
>
   <Header bind:transferMode />

   <div class="flex-1 flex flex-col min-h-0">
      <div class="p-4 pb-0 shrink-0">
         <Editor onStash={handleStash} />
      </div>

      <div class="h-px bg-border/50 my-2 mx-4 shrink-0"></div>

      <Queue {transferMode} {refreshTrigger} />
   </div>
</main>
