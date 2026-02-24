import { render } from '@testing-library/react';
import React from 'react';

// Common providers go here
const AllTheProviders = ({ children }: { children: React.ReactNode }) => {
    return (
        <>
            {children}
        </>
    );
};

const customRender = (
    ui: React.ReactElement,
    options?: Parameters<typeof render>[1]
) =>
    render(ui, { wrapper: AllTheProviders, ...options });

// re-export everything
export * from '@testing-library/react';

// override render method
export { customRender as render };
