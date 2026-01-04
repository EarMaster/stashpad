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

    node.addEventListener('keydown', handleKeydown);

    // Focus the first element initially
    const first = node.querySelector(focusableElementsString) as HTMLElement;
    if (first) {
        first.focus();
    }

    return {
        destroy() {
            node.removeEventListener('keydown', handleKeydown);
        }
    };
}
