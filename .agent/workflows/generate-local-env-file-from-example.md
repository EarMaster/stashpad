---
description: Safely create a local .env file from .env.example
---

1. **Check for .env.example**:
   - Ensure the example file exists.
   // turbo
   - Run `test -f .env.example && echo "✅ Found .env.example" || echo "❌ .env.example not found"`

2. **Copy to .env.local**:
   - Create your local config without overwriting if it exists (using -n).
   // turbo
   - Run `cp -n .env.example .env.local || echo ".env.local already exists"`

3. **Validate**:
   - Open `.env.local` and replace all placeholder values.
   - Example: `YOUR_API_KEY_HERE` → `abc123...`

4. **Pro Tips**:
   - Always add `.env.local` to your `.gitignore`.
   - Never commit real secrets to `.env.example`.
   - Use `git secret` or Vercel Environment Variables for production secrets.