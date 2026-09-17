// SPDX-License-Identifier: AGPL-3.0-only

export function trapFocus(node: HTMLElement) {
    const focusableElementsString =
        'a[href], area[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), iframe, object, embed, [tabindex="0"], [contenteditable]';

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Tab') {
            const focusableElements = Array.from(
                node.querySelectorAll(focusableElementsString)
            ) as HTMLElement[];

            if (focusableElements.length === 0) {
                event.preventDefault();
                return;
            }

            const firstElement = focusableElements[0];
            const lastElement = focusableElements[focusableElements.length - 1];

            if (event.shiftKey) {
                if (document.activeElement === firstElement) {
                    event.preventDefault();
                    lastElement.focus();
                }
            } else {
                if (document.activeElement === lastElement) {
                    event.preventDefault();
                    firstElement.focus();
                }
            }
        }
    }

    // Captured before focus is moved inside, so it can be handed back on teardown.
    // Doing this in the action rather than an effect is deliberate: an effect runs
    // after the DOM update, by which point the first element below already has focus
    // and the element that opened the panel is no longer `activeElement`.
    const previouslyFocused = document.activeElement as HTMLElement | null;

    node.addEventListener('keydown', handleKeydown);

    // Focus the first element initially
    const first = node.querySelector(focusableElementsString) as HTMLElement;
    if (first) {
        first.focus();
    }

    return {
        destroy() {
            node.removeEventListener('keydown', handleKeydown);
            // Only if focus is still inside the panel being torn down. Something else
            // may have claimed it in the meantime, and stealing it back would be worse
            // than leaving it. A trigger that has itself been removed - the delete
            // button of the stash just deleted - is no longer connected, and focusing
            // it would do nothing.
            if (
                previouslyFocused?.isConnected &&
                node.contains(document.activeElement)
            ) {
                previouslyFocused.focus();
            }
        }
    };
}
