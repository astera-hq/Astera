import { act, render, renderHook, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import NotificationBell from '@/components/NotificationBell';
import { useTransactionSimulation } from '@/hooks/useTransactionSimulation';

jest.mock('@/lib/notification-preferences', () => ({ isInAppEnabled: jest.fn(() => true) }));
jest.mock('@/lib/notifications', () => ({
  notificationService: { subscribe: jest.fn(() => jest.fn()) },
}));

describe('useTransactionSimulation', () => {
  it('re-simulates when transaction inputs change while enabled remains true', async () => {
    const first = jest.fn(async () => ({
      minResourceFee: 100,
      instructions: 50,
      feeInXlm: 0.00001,
    }));
    const second = jest.fn(async () => ({
      minResourceFee: 300,
      instructions: 150,
      feeInXlm: 0.00003,
    }));
    const { result, rerender } = renderHook(
      ({ simulate }) => useTransactionSimulation(simulate, true),
      { initialProps: { simulate: first } },
    );
    await waitFor(() => expect(result.current.status).toBe('success'));
    expect(first).toHaveBeenCalledTimes(1);

    rerender({ simulate: second });
    await waitFor(() => expect(second).toHaveBeenCalledTimes(1));
    expect(result.current.feeEstimate).toEqual({
      minResourceFee: 300,
      instructions: 150,
      feeInXlm: 0.00003,
    });
  });
});

describe('NotificationBell keyboard behavior', () => {
  it('moves focus into the dialog, closes on Escape, and restores trigger focus', async () => {
    render(<NotificationBell />);
    const trigger = screen.getByRole('button');
    trigger.focus();
    await userEvent.click(trigger);
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveFocus();

    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it('keeps focus inside an empty dialog when tabbing', async () => {
    render(<NotificationBell />);
    await userEvent.click(screen.getByRole('button'));
    const dialog = screen.getByRole('dialog');
    act(() => dialog.focus());
    await userEvent.tab();
    expect(dialog).toHaveFocus();
  });
});
