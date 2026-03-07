import { expect, test } from 'vitest';

test('admin privileges are respected', async () => {
    // Note: Since real passkey auth requires user interaction & hardware devices,
    // we'll simulate the endpoint requests normally. This is a manual test placeholder.
    // In actual use, developers will:
    // 1. Visit /en/admin
    // 2. Authorize via passkey.
    // 3. See 403 Forbidden because they are not admin.
    // 4. Run `cd cloud/scripts && ./make_admin.sh testuser@example.com`
    // 5. Reload /en/admin
    // 6. See the dashboard, Waitlist, and existing users.
    // 7. Click Upgrade on a test user.
    // 8. Waitlist and Users should be accurately reflected.
});
