import { render, screen, fireEvent } from '@testing-library/react';
import OnboardingModal, { isFirstTimeUser, resetOnboardingFlag } from '../OnboardingModal';

describe('OnboardingModal', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('renders nothing when isOpen is false', () => {
    const { container } = render(<OnboardingModal isOpen={false} onClose={jest.fn()} />);
    expect(container.innerHTML).toBe('');
  });

  it('marks the user as returning and closes on "Skip tour" from the role screen', () => {
    const onClose = jest.fn();
    render(<OnboardingModal isOpen={true} onClose={onClose} />);
    expect(isFirstTimeUser()).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: /skip tour/i }));

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(isFirstTimeUser()).toBe(false);
  });

  it('walks through the full SME (borrower) flow and completes it', () => {
    const onClose = jest.fn();
    render(<OnboardingModal isOpen={true} onClose={onClose} />);

    fireEvent.click(screen.getByRole('button', { name: /I am an SME seeking invoice financing/i }));
    expect(screen.getByText('Welcome to Astera')).toBeInTheDocument();
    expect(screen.getByLabelText('Step 1 of 5')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /^next$/i })); // -> Connect Your Wallet
    expect(screen.getByText('Connect Your Wallet')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /^next$/i })); // -> Create Your First Invoice
    fireEvent.click(screen.getByRole('button', { name: /^next$/i })); // -> Wait for Verification
    fireEvent.click(screen.getByRole('button', { name: /^next$/i })); // -> Receive Your Funds
    expect(screen.getByText('Receive Your Funds')).toBeInTheDocument();
    expect(screen.getByLabelText('Step 5 of 5')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /get started/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /get started/i }));

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(isFirstTimeUser()).toBe(false);
  });

  it('supports going back to the previous step, and back to role selection from step 1', () => {
    render(<OnboardingModal isOpen={true} onClose={jest.fn()} />);

    fireEvent.click(
      screen.getByRole('button', { name: /I am an investor looking to earn yield/i }),
    );
    fireEvent.click(screen.getByRole('button', { name: /^next$/i })); // -> step 2
    expect(screen.getByLabelText('Step 2 of 5')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /previous/i }));
    expect(screen.getByLabelText('Step 1 of 5')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /^back$/i }));
    expect(screen.getByText(/tell us how you/i)).toBeInTheDocument();
  });

  it('highlights the wallet-connect element on the "Connect Your Wallet" step when present', () => {
    document.body.innerHTML = '<button data-onboarding-id="wallet-connect">Connect</button>';
    // jsdom returns a zero-size rect from getBoundingClientRect by default; stub a real one.
    const target = document.querySelector('[data-onboarding-id="wallet-connect"]') as HTMLElement;
    target.getBoundingClientRect = () => ({ top: 10, left: 20, width: 100, height: 40 }) as DOMRect;

    render(<OnboardingModal isOpen={true} onClose={jest.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: /I am an SME seeking invoice financing/i }));
    fireEvent.click(screen.getByRole('button', { name: /^next$/i })); // -> "Connect Your Wallet"

    const spotlight = document.querySelector('[aria-hidden="true"].ring-brand-gold') as HTMLElement;
    expect(spotlight).not.toBeNull();
    expect(spotlight.style.width).toBe('112px');
  });

  it('falls back to the flat backdrop when the highlight target is not on the page', () => {
    render(<OnboardingModal isOpen={true} onClose={jest.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: /I am an SME seeking invoice financing/i }));
    fireEvent.click(screen.getByRole('button', { name: /^next$/i })); // -> "Connect Your Wallet"

    expect(document.querySelector('[aria-hidden="true"].ring-brand-gold')).toBeNull();
  });

  it('resetOnboardingFlag makes the user "first time" again', () => {
    localStorage.setItem('astera-onboarding-completed', 'true');
    expect(isFirstTimeUser()).toBe(false);
    resetOnboardingFlag();
    expect(isFirstTimeUser()).toBe(true);
  });
});
