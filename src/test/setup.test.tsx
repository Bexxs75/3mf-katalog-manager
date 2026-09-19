import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { makeModelFile } from './factories';

describe('test infrastructure', () => {
  it('renders with jsdom + RTL', () => {
    render(<div>ok</div>);
    expect(screen.getByText('ok')).toBeInTheDocument();
  });

  it('factories produce a valid ModelFile', () => {
    const model = makeModelFile({ name: 'Custom' });
    expect(model.name).toBe('Custom');
    expect(model.id).toBe('model-1');
  });
});
