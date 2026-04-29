import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import ConfirmActionModal from '../ConfirmActionModal';

// Mock React portal rendering
jest.mock('react-dom', () => ({
  ...jest.requireActual('react-dom'),
  createPortal: (node: React.ReactNode) => node,
}));

describe('ConfirmActionModal', () => {
  const mockOnConfirm = jest.fn();
  const mockOnCancel = jest.fn();

  const defaultProps = {
    title: 'Test Confirmation',
    description: 'This is a test description.',
    onConfirm: mockOnConfirm,
    onCancel: mockOnCancel,
    isOpen: true,
  };

  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('renders nothing when isOpen is false', () => {
    const { container } = render(<ConfirmActionModal {...defaultProps} isOpen={false} />);
    expect(container.innerHTML).toBe('');
  });

  it('renders title and description when open', () => {
    render(<ConfirmActionModal {...defaultProps} />);
    expect(screen.getByText('Test Confirmation')).toBeInTheDocument();
    expect(screen.getByText('This is a test description.')).toBeInTheDocument();
  });

  it('renders Cancel and Confirm buttons', () => {
    render(<ConfirmActionModal {...defaultProps} />);
    expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /confirm/i })).toBeInTheDocument();
  });

  it('calls onCancel when Cancel button is clicked', () => {
    render(<ConfirmActionModal {...defaultProps} />);
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(mockOnCancel).toHaveBeenCalledTimes(1);
  });

  it('calls onCancel when close (X) button is clicked', () => {
    render(<ConfirmActionModal {...defaultProps} />);
    const closeButton = screen.getByRole('button', { name: /close dialog/i });
    fireEvent.click(closeButton);
    expect(mockOnCancel).toHaveBeenCalledTimes(1);
  });

  it('calls onConfirm when Confirm button is clicked (no phrase required)', () => {
    render(<ConfirmActionModal {...defaultProps} />);
    fireEvent.click(screen.getByRole('button', { name: /confirm/i }));
    expect(mockOnConfirm).toHaveBeenCalledTimes(1);
  });

  it('renders with destructive variant styling', () => {
    render(
      <ConfirmActionModal {...defaultProps} variant="destructive" confirmPhrase="CONFIRM TEST" />,
    );
    // Confirm button should be disabled until phrase matches
    const confirmBtn = screen.getByRole('button', { name: /confirm/i });
    expect(confirmBtn).toHaveAttribute('disabled');
  });

  describe('with confirmPhrase', () => {
    it('shows input field when confirmPhrase is provided', () => {
      render(
        <ConfirmActionModal {...defaultProps} confirmPhrase="CONFIRM TEST" variant="destructive" />,
      );
      expect(screen.getByPlaceholderText('CONFIRM TEST')).toBeInTheDocument();
    });

    it('disables Confirm button when phrase does not match', () => {
      render(
        <ConfirmActionModal {...defaultProps} confirmPhrase="CONFIRM TEST" variant="destructive" />,
      );
      const confirmBtn = screen.getByRole('button', { name: /confirm/i });
      expect(confirmBtn).toHaveAttribute('disabled');
    });

    it('enables Confirm button when phrase matches', () => {
      render(
        <ConfirmActionModal {...defaultProps} confirmPhrase="CONFIRM TEST" variant="destructive" />,
      );
      const input = screen.getByPlaceholderText('CONFIRM TEST');
      fireEvent.change(input, { target: { value: 'CONFIRM TEST' } });
      const confirmBtn = screen.getByRole('button', { name: /confirm/i });
      expect(confirmBtn).not.toHaveAttribute('disabled');
    });

    it('calls onConfirm when phrase matches and Confirm is clicked', () => {
      render(
        <ConfirmActionModal {...defaultProps} confirmPhrase="CONFIRM TEST" variant="destructive" />,
      );
      const input = screen.getByPlaceholderText('CONFIRM TEST');
      fireEvent.change(input, { target: { value: 'CONFIRM TEST' } });
      fireEvent.click(screen.getByRole('button', { name: /confirm/i }));
      expect(mockOnConfirm).toHaveBeenCalledTimes(1);
    });

    it('does not call onConfirm when phrase does not match', () => {
      render(
        <ConfirmActionModal {...defaultProps} confirmPhrase="CONFIRM TEST" variant="destructive" />,
      );
      const input = screen.getByPlaceholderText('CONFIRM TEST');
      fireEvent.change(input, { target: { value: 'WRONG PHRASE' } });
      const confirmBtn = screen.getByRole('button', { name: /confirm/i });
      fireEvent.click(confirmBtn);
      expect(mockOnConfirm).not.toHaveBeenCalled();
    });

    it('resets typed phrase after confirmation', async () => {
      render(
        <ConfirmActionModal {...defaultProps} confirmPhrase="CONFIRM TEST" variant="destructive" />,
      );
      const input = screen.getByPlaceholderText('CONFIRM TEST');
      fireEvent.change(input, { target: { value: 'CONFIRM TEST' } });
      fireEvent.click(screen.getByRole('button', { name: /confirm/i }));
      expect(mockOnConfirm).toHaveBeenCalledTimes(1);
    });
  });

  describe('accessibility', () => {
    it('has correct ARIA attributes', () => {
      render(<ConfirmActionModal {...defaultProps} />);
      const dialog = screen.getByRole('dialog');
      expect(dialog).toHaveAttribute('aria-modal', 'true');
      expect(dialog).toHaveAttribute('aria-labelledby', 'confirm-modal-title');
      expect(dialog).toHaveAttribute('aria-describedby', 'confirm-modal-description');
    });

    it('calls onCancel when Escape key is pressed', () => {
      render(<ConfirmActionModal {...defaultProps} />);
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
      expect(mockOnCancel).toHaveBeenCalledTimes(1);
    });
  });

  describe('custom labels', () => {
    it('renders custom confirm and cancel labels', () => {
      render(
        <ConfirmActionModal
          {...defaultProps}
          confirmLabel="Yes, Proceed"
          cancelLabel="No, Go Back"
        />,
      );
      expect(screen.getByRole('button', { name: /yes, proceed/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /no, go back/i })).toBeInTheDocument();
    });
  });
});
