# Testing Guide for Stashpad

This document provides comprehensive guidance on testing practices for the Stashpad project.

## Table of Contents

1. [Running Tests](#running-tests)
2. [Frontend Testing (TypeScript/Svelte)](#frontend-testing)
3. [Backend Testing (Rust)](#backend-testing)
4. [Writing New Tests](#writing-new-tests)
5. [Code Coverage](#code-coverage)
6. [CI/CD Integration](#cicd-integration)
7. [Best Practices](#best-practices)

---

## Running Tests

### Frontend Tests

```powershell
# Run all tests once
npm run test

# Run tests in watch mode (useful during development)
npm run test:watch

# Run tests with UI (visual interface)
npm run test:ui

# Run tests with coverage report
npm run test:coverage

# Run only unit tests
npm run test:unit

# Run only integration tests
npm run test:integration
```

### Backend Tests

```powershell
# Run all Rust tests
cd src-tauri
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests in release mode (faster)
cargo test --release
```

---

## Frontend Testing

### Technology Stack

- **Framework**: [Vitest](https://vitest.dev/) v4.0+
- **Testing Library**: [@testing-library/svelte](https://testing-library.com/docs/svelte-testing-library/intro)
- **Environment**: jsdom (for DOM testing)
- **Mocking**: Vitest's built-in mocking

### Test Structure

All frontend tests are located in `__tests__` directories next to the code they test:

```
src/lib/
├── utils/
│   ├── __tests__/
│   │   ├── markdown.test.ts
│   │   ├── format.test.ts
│   │   ├── date.test.ts
│   │   └── version.test.ts
│   ├── markdown.ts
│   ├── format.ts
│   └── ...
├── services/
│   ├── __tests__/
│   │   └── desktop-adapter.test.ts
│   └──desktop-adapter.ts
└── components/
    ├── __tests__/
    │   └── (component tests would go here)
    └── ...
```

### Testing Utilities

#### Example: Testing Pure Functions

```typescript
import { describe, it, expect } from 'vitest';
import { formatBytes } from '../format';

describe('formatBytes', () => {
  it('should format bytes correctly', () => {
    expect(formatBytes(1024)).toBe('1 KB');
    expect(formatBytes(1024 * 1024)).toBe('1 MB');
  });
});
```

#### Example: Testing with Mocked Dependencies

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { DesktopStorageAdapter } from '../desktop-adapter';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';

describe('DesktopStorageAdapter', () => {
  let adapter: DesktopStorageAdapter;
  let mockInvoke: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    adapter = new DesktopStorageAdapter();
    mockInvoke = invoke as ReturnType<typeof vi.fn>;
    mockInvoke.mockClear();
  });

  it('should call load_stashes command', async () => {
    mockInvoke.mockResolvedValue([]);
    await adapter.loadStashes();
    expect(mockInvoke).toHaveBeenCalledWith('load_stashes');
  });
});
```

#### Global Mocks

All tests have access to mocked Tauri APIs via `vitest.setup.ts`:

- `@tauri-apps/api/core` - Mocked `invoke` function
- `@tauri-apps/api/event` - Mocked event listeners

---

## Backend Testing

### Testing Philosophy

Rust tests should cover:

1. **Unit Tests**: Individual functions and methods
2. **Integration Tests:** Complete workflows with in-memory databases
3. **Edge Cases**: Error handling, boundary conditions

### Example: Unit Tests for Commands

Add tests at the end of `src-tauri/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_app_dir() {
        let dir = get_app_dir();
        assert!(dir.ends_with(".stashpad"));
    }

    #[test]
    fn test_validate_settings() {
        let mut settings = Settings::default();
        settings.new_stash_position = "invalid".to_string();
        
        let validated = validate_settings(settings);
        assert_eq!(validated.new_stash_position, "top");
    }
}
```

### Example: Database Tests

Add tests in `src-tauri/src/db.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_db() -> DbManager {
        DbManager::new_in_memory().expect("Failed to create test DB")
    }

    #[test]
    fn test_save_and_get_stash() {
        let mut db = create_test_db();
        
        let stash = StashItem {
            id: "test-123".to_string(),
            content: "test content".to_string(),
            attachments: vec![],
            files: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            context_id: Some("default".to_string()),
            completed: false,
            completed_at: None,
        };

        db.save_stash(&stash, None).expect("Failed to save stash");
        
        let stashes = db.get_stashes().expect("Failed to get stashes");
        assert_eq!(stashes.len(), 1);
        assert_eq!(stashes[0].id, "test-123");
    }
}
```

---

## Writing New Tests

### Guidelines

1. **Name Tests Descriptively**: Test names should clearly describe what they're testing
   - Good: `test_save_asset_with_valid_file`
   - Bad: `test1`

2. **Follow AAA Pattern**: Arrange, Act, Assert
   ```typescript
   it('should format bytes correctly', () => {
     // Arrange
     const bytes = 1024;
     
     // Act
     const result = formatBytes(bytes);
     
     // Assert
     expect(result).toBe('1 KB');
   });
   ```

3. **Test One Thing**: Each test should verify one specific behavior

4. **Use Descriptive Assertions**: Make failures easy to understand
   ```typescript
   // Good
   expect(result).toBe('1 KB');
   
   // Better with message
   expect(result).toBe('1 KB'); // If using jest-dom matchers
   ```

5. **Mock External Dependencies**: Don't make real API calls or file system operations in tests

### When to Add Tests

- **New Features**: Add tests before or alongside new code (TDD encouraged)
- **Bug Fixes**: Add a regression test that reproduces the bug, then fix it
- **Refactoring**: Ensure existing tests pass after refactoring

---

## Code Coverage

### Viewing Coverage

```powershell
npm run test:coverage
```

This generates a coverage report in:
- Terminal output (text summary)
- `coverage/index.html` (interactive HTML report)

### Coverage Goals

- **Utilities**: Aim for >90% coverage
- **Services**: Aim for >80% coverage
- **Components**: Aim for >70% coverage
- **Overall**: Maintain >75% coverage

### Ignoring Files from Coverage

Files automatically excluded (see `vitest.config.ts`):
- Test files (`*.test.ts`, `*.spec.ts`)
- Mock files (`__mocks__/**`)
- Configuration files (`*.config.ts`)

---

## CI/CD Integration

Tests run automatically on:
- Every push to `main` branch
- Every pull request
- Manual workflow dispatch

### GitHub Actions Workflow

Located at `.github/workflows/test.yml` (to be created):

```yaml
name: Tests

on:
  push:
    branches: [main]
  pull_request:

jobs:
  frontend-tests:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
      - run: npm install
      - run: npm run test:coverage
      
  backend-tests:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
      - run: cd src-tauri && cargo test
```

---

## Best Practices

### DO:

✅ Write tests for all new features  
✅ Keep tests simple and focused  
✅ Use meaningful test descriptions  
✅ Mock external dependencies  
✅ Test edge cases and error conditions  
✅ Run tests before committing  
✅ Keep test code clean and maintainable  

### DON'T:

❌ Skip tests because "it's simple code"  
❌ Write tests that depend on execution order  
❌ Make tests that require manual setup  
❌ Test implementation details (test behavior, not internals)  
❌ Leave failing tests in the codebase  
❌ Copy-paste test code without understanding it  

### Common Pitfalls

1. **Locale-Dependent Tests**: Tests that rely on dates or localization should handle multiple locales
   ```typescript
   // Bad: Will fail in non-English locales
   expect(formattedDate).toBe('Dec 2025');
   
   // Good: Flexible assertion
   expect(formattedDate).toContain('2025');
   ```

2. **File API Mocking**: jsdom doesn't fully support File APIs
   ```typescript
   // Mock arrayBuffer for File objects in tests
   mockFile.arrayBuffer = vi.fn().mockResolvedValue(buffer);
   ```

3. **Async Testing**: Always await async operations
   ```typescript
   // Bad
   it('should load data', () => {
     fetchData(); // Not awaited!
     expect(data).toBeDefined(); // Will fail
   });
   
   // Good
   it('should load data', async () => {
     await fetchData();
     expect(data).toBeDefined();
   });
   ```

---

## Resources

- [Vitest Documentation](https://vitest.dev/)
- [Testing Library Best Practices](https://testing-library.com/docs/queries/about#priority)
- [Rust Testing Documentation](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Tauri Testing Guide](https://tauri.app/v1/guides/testing/)

---

## Getting Help

If you encounter issues with tests:

1. Check this guide and the links above
2. Look at existing tests for patterns
3. Run tests with `--help` to see all options
4. Ask the team for help!

Remember: **Good tests are documentation that never lies.** Write tests that make your code easier to understand and maintain.
