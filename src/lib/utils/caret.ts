// Based on https://github.com/component/textarea-caret
// SPDX-License-Identifier: MIT

export interface CaretCoordinates {
    top: number;
    left: number;
    height: number;
    lineHeight: number;
}

const properties = [
    'direction',
    'boxSizing',
    'width',
    'height',
    'overflowX',
    'overflowY',
    'borderTopWidth',
    'borderRightWidth',
    'borderBottomWidth',
    'borderLeftWidth',
    'borderStyle',
    'paddingTop',
    'paddingRight',
    'paddingBottom',
    'paddingLeft',
    'fontStyle',
    'fontVariant',
    'fontWeight',
    'fontStretch',
    'fontSize',
    'fontSizeAdjust',
    'lineHeight',
    'fontFamily',
    'textAlign',
    'textTransform',
    'textIndent',
    'textDecoration',
    'letterSpacing',
    'wordSpacing',
    'tabSize',
    'MozTabSize',
    'wordBreak',
    'overflowWrap',
    'whiteSpace',
    'breakInside'
];

interface CaretOptions {
    appendTo?: HTMLElement;
}

export function getCaretCoordinates(element: HTMLTextAreaElement | HTMLInputElement, position: number, options?: CaretOptions): CaretCoordinates {
    // The mirror div will become a body child.
    // We must ensure it doesn't affect the layout.
    const div = document.createElement('div');
    div.id = 'input-textarea-caret-position-mirror-div';

    const container = options?.appendTo || document.body;
    container.appendChild(div);

    const style = div.style;
    const computed = window.getComputedStyle(element);

    style.whiteSpace = 'pre-wrap';
    if (element.nodeName !== 'INPUT')
        style.wordWrap = 'break-word';  // only for textarea-s

    // Position off-screen
    style.position = 'absolute';  // required to return coordinates properly
    style.visibility = 'hidden';  // not 'display: none' because we want rendering

    // Transfer the element's properties to the div
    properties.forEach(prop => {
        // @ts-ignore
        style[prop] = computed[prop];
    });

    // Adjust width to account for scrollbars
    if (window.getComputedStyle(element).boxSizing === 'border-box') {
        style.width = String(parseInt(computed.width!) -
            (parseInt(computed.borderLeftWidth!) + parseInt(computed.borderRightWidth!))) + 'px'; // Border-box excludes border from inner content calculation? No. 
        // We want the inner width.
        // If border-box, width includes border.
        // We want to exclude scrollbar.
    } else {
        style.width = computed.width;
    }

    // Better width calculation to match clientWidth (which excludes scrollbar)
    // We enforce border-box on the mirror to make it easy.
    style.boxSizing = 'border-box';
    // clientWidth includes padding but excludes border and scrollbar.
    // If we set width to clientWidth, and border to 0, and keep padding...
    // The content area will be clientWidth - padding.
    // The source element content area is clientWidth - padding.
    // Matches!
    style.width = `${element.clientWidth}px`;
    style.borderWidth = '0px';
    style.margin = '0px'; // Margin doesn't matter for content width but good to clear.

    style.overflow = 'hidden';  // for Chrome to not render scrollbars;

    div.textContent = element.value.substring(0, position);

    if (element.nodeName === 'INPUT') {
        div.textContent = div.textContent.replace(/\s/g, '\u00a0');
    }

    const span = document.createElement('span');
    span.textContent = element.value.substring(position) || '.';
    div.appendChild(span);

    // Calculate line height strictly
    const lhDiv = document.createElement('div');
    lhDiv.style.margin = "0";
    lhDiv.style.padding = "0";
    lhDiv.textContent = "Hg";
    div.appendChild(lhDiv);
    const calculatedLineHeight = lhDiv.offsetHeight;
    div.removeChild(lhDiv);

    // Get coordinates
    const spanRect = span.getClientRects()[0]; // Use first rect to target the cursor start position
    const divRect = div.getBoundingClientRect();

    const coordinates = {
        top: spanRect.top - divRect.top,
        left: spanRect.left - divRect.left,
        height: spanRect.height,
        lineHeight: calculatedLineHeight
    };

    container.removeChild(div);

    return coordinates;
}
