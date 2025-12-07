---
description: Find and fix accessibility violations
---

1. **Install axe-core**:
   - Use the CLI tool for quick audits.
   // turbo
   - Run `npm install -g @axe-core/cli`

2. **Run Audit**:
   - Check a specific URL. Replace with your local or prod URL.
   // turbo
   - Run `axe http://localhost:3000`

3. **Pro Tips**:
   - Use the **Lighthouse** Accessibility audit in Chrome DevTools for a visual report.
   - Aim for 100% accessibility score; it helps SEO too.