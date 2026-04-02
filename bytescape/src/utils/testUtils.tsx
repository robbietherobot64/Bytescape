import { render } from '@testing-library/react';
import React from 'react';

/**
 * PURPOSE
 * Shared test render utilities and exports for component tests.
 *
 * HOW IT WORKS
 * Wraps Testing Library `render()` with a single app-level provider wrapper,
 * then re-exports Testing Library symbols for convenience.
 *
 * RELATED FILES
 * - src/setupTests.ts
 * - component/unit test files importing from this module
 */
// Common providers go here
const AllTheProviders = ({ children }: { children: React.ReactNode }) => {
    return (
        <>
            {children}
        </>
    );
};

/**
 * Test-friendly `render()` that injects shared providers used by app components.
 */
const customRender = (
    ui: React.ReactElement,
    options?: Parameters<typeof render>[1]
) =>
    render(ui, { wrapper: AllTheProviders, ...options });

/** Re-exports Testing Library helpers so tests can import from one module. */
export * from '@testing-library/react';

/** Overrides exported `render` with provider-aware `customRender`. */
export { customRender as render };
