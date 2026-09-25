import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SpoolToast } from './SpoolToast';

afterEach(() => vi.useRealTimers());

function renderToast(location: string | null, onChangeLocation = vi.fn().mockResolvedValue(undefined)) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const onDone = vi.fn();
  render(
    <LanguageProvider>
      <SpoolToast label="PLA Schwarz" location={location} knownLocations={['Regal 1']} onChangeLocation={onChangeLocation} onDone={onDone} />
    </LanguageProvider>,
  );
  return { onDone, onChangeLocation };
}

describe('SpoolToast', () => {
  it('names the home location and closes by itself', () => {
    vi.useFakeTimers();
    const { onDone } = renderToast('Regal 2');
    expect(screen.getByRole('status')).toHaveTextContent('PLA Schwarz');
    expect(screen.getByRole('status')).toHaveTextContent('zurück nach Regal 2');
    act(() => { vi.advanceTimersByTime(5000); });
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('says "back to storage" without a home location', () => {
    renderToast(null);
    expect(screen.getByRole('status')).toHaveTextContent('zurück ins Lager');
  });

  it('lets the user change the location and stays open while editing', async () => {
    vi.useFakeTimers();
    const { onDone, onChangeLocation } = renderToast('Regal 2');
    fireEvent.click(screen.getByRole('button', { name: 'Ändern' }));
    act(() => { vi.advanceTimersByTime(6000); });
    expect(onDone).not.toHaveBeenCalled();
    fireEvent.change(screen.getByPlaceholderText('Lagerort'), { target: { value: 'Trockenbox' } });
    vi.useRealTimers();
    fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(onChangeLocation).toHaveBeenCalledWith('Trockenbox');
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it('shows a plain message without a change action when no location callback is given', () => {
    vi.useFakeTimers();
    localStorage.setItem('3mf-katalog-language', 'de');
    const onDone = vi.fn();
    render(
      <LanguageProvider>
        <SpoolToast label="2 Spulen PETG · Rot angelegt" location={null} knownLocations={[]} onDone={onDone} />
      </LanguageProvider>,
    );
    expect(screen.getByRole('status')).toHaveTextContent('2 Spulen PETG · Rot angelegt');
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.getByRole('status')).not.toHaveTextContent('zurück');
    act(() => { vi.advanceTimersByTime(5000); });
    expect(onDone).toHaveBeenCalledTimes(1);
  });
});
