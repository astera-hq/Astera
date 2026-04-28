'use client';

import { useState, useEffect, useRef } from 'react';

interface OnboardingModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const ONBOARDING_STORAGE_KEY = 'astera-onboarding-completed';

type UserRole = 'sme' | 'investor' | null;

const SME_STEPS = [
  {
    title: 'Welcome to Astera',
    content:
      'Astera lets you get paid early on your outstanding invoices. Tokenize your invoices on the Stellar blockchain and receive USDC directly to your wallet.',
  },
  {
    title: 'Connect Your Wallet',
    content:
      "Connect your Freighter wallet to get started. Freighter is a browser extension for the Stellar network — install it from the Chrome Web Store if you haven't already.",
  },
  {
    title: 'Create Your First Invoice',
    content:
      "Go to the Invoice page and fill in your debtor's name, invoice amount, and due date. You'll need your debtor's information and the amount owed to you.",
  },
  {
    title: 'Wait for Verification',
    content:
      'Our oracle will verify your invoice details. Verified invoices become eligible for funding from the liquidity pool. This typically takes a short time.',
  },
  {
    title: 'Receive Your Funds',
    content:
      "Once funded, you'll receive USDC directly to your connected wallet. Repay the invoice when your customer settles — and you're done!",
  },
];

const INVESTOR_STEPS = [
  {
    title: 'Welcome to Astera',
    content:
      'Deposit stablecoins to earn yield from invoice financing. Your funds are deployed to verified SME invoices and you earn interest on repayment.',
  },
  {
    title: 'Connect Your Wallet',
    content:
      "Connect your Freighter wallet to access the investor dashboard. Freighter is a Stellar browser extension — install it if you haven't already.",
  },
  {
    title: 'Deposit USDC',
    content:
      'Go to the Invest page and deposit USDC into the liquidity pool. You will receive share tokens representing your proportional ownership of the pool.',
  },
  {
    title: 'Your Funds Go to Work',
    content:
      'The pool deploys your funds to verified SME invoices. Your share of the pool grows as invoices are repaid with interest.',
  },
  {
    title: 'Earn Yield Automatically',
    content:
      'Withdraw anytime your funds are available. Claim accrued yield at any time from the dashboard. Your earnings are proportional to your share of the pool.',
  },
];

export default function OnboardingModal({ isOpen, onClose }: OnboardingModalProps) {
  const [role, setRole] = useState<UserRole>(null);
  const [currentStep, setCurrentStep] = useState(0);
  const modalRef = useRef<HTMLDivElement>(null);
  const firstFocusableRef = useRef<HTMLButtonElement>(null);

  const steps = role === 'sme' ? SME_STEPS : role === 'investor' ? INVESTOR_STEPS : [];
  const totalSteps = steps.length;

  useEffect(() => {
    if (isOpen && firstFocusableRef.current) {
      firstFocusableRef.current.focus();
    }
  }, [isOpen]);

  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };

    const handleTab = (e: KeyboardEvent) => {
      if (!isOpen || !modalRef.current) return;

      const focusableElements = modalRef.current.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      const firstElement = focusableElements[0] as HTMLElement;
      const lastElement = focusableElements[focusableElements.length - 1] as HTMLElement;

      if (e.key === 'Tab') {
        if (e.shiftKey) {
          if (document.activeElement === firstElement) {
            lastElement.focus();
            e.preventDefault();
          }
        } else {
          if (document.activeElement === lastElement) {
            firstElement.focus();
            e.preventDefault();
          }
        }
      }
    };

    document.addEventListener('keydown', handleEscape);
    document.addEventListener('keydown', handleTab);

    return () => {
      document.removeEventListener('keydown', handleEscape);
      document.removeEventListener('keydown', handleTab);
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const handleNext = () => {
    if (currentStep < totalSteps - 1) {
      setCurrentStep(currentStep + 1);
    } else {
      handleComplete();
    }
  };

  const handlePrevious = () => {
    if (currentStep > 0) {
      setCurrentStep(currentStep - 1);
    }
  };

  const handleComplete = () => {
    localStorage.setItem(ONBOARDING_STORAGE_KEY, 'true');
    onClose();
  };

  const handleSkip = () => {
    localStorage.setItem(ONBOARDING_STORAGE_KEY, 'true');
    onClose();
  };

  const handleRoleSelect = (selected: UserRole) => {
    setRole(selected);
    setCurrentStep(0);
  };

  return (
    <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div
        ref={modalRef}
        className="bg-brand-dark border border-brand-border rounded-2xl max-w-lg w-full max-h-[80vh] overflow-y-auto"
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
      >
        <div className="p-6">
          {/* Header */}
          <div className="flex items-center justify-between mb-6">
            <h2 id="onboarding-title" className="text-xl font-bold">
              {role === null ? 'Get Started' : steps[currentStep]?.title}
            </h2>
            <button
              ref={firstFocusableRef}
              onClick={onClose}
              className="text-brand-muted hover:text-white transition-colors"
              aria-label="Close onboarding"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </button>
          </div>

          {/* Role selection */}
          {role === null ? (
            <div className="space-y-4">
              <p className="text-brand-muted text-sm">
                Tell us how you&apos;ll use Astera so we can show you the right guide.
              </p>
              <div className="grid grid-cols-2 gap-4">
                <button
                  onClick={() => handleRoleSelect('sme')}
                  className="flex flex-col items-center gap-3 p-5 bg-brand-card border border-brand-border rounded-xl hover:border-brand-gold transition-colors text-left"
                  aria-label="I am an SME seeking invoice financing"
                >
                  <span className="text-3xl" aria-hidden="true">
                    🏢
                  </span>
                  <div>
                    <p className="font-semibold text-sm">SME / Business</p>
                    <p className="text-xs text-brand-muted mt-1">I want to finance my invoices</p>
                  </div>
                </button>
                <button
                  onClick={() => handleRoleSelect('investor')}
                  className="flex flex-col items-center gap-3 p-5 bg-brand-card border border-brand-border rounded-xl hover:border-brand-gold transition-colors text-left"
                  aria-label="I am an investor looking to earn yield"
                >
                  <span className="text-3xl" aria-hidden="true">
                    💰
                  </span>
                  <div>
                    <p className="font-semibold text-sm">Investor</p>
                    <p className="text-xs text-brand-muted mt-1">
                      I want to deposit and earn yield
                    </p>
                  </div>
                </button>
              </div>
              <div className="flex justify-end pt-2">
                <button
                  onClick={handleSkip}
                  className="px-4 py-2 text-sm text-brand-muted hover:text-white transition-colors"
                >
                  Skip tour
                </button>
              </div>
            </div>
          ) : (
            <>
              {/* Step indicator */}
              <div
                className="flex items-center justify-center mb-6"
                aria-label={`Step ${currentStep + 1} of ${totalSteps}`}
              >
                <div className="flex items-center space-x-2">
                  {steps.map((_, index) => (
                    <div
                      key={index}
                      aria-hidden="true"
                      className={`h-2 rounded-full transition-all duration-300 ${
                        index === currentStep
                          ? 'w-8 bg-brand-gold'
                          : index < currentStep
                            ? 'w-2 bg-brand-gold/60'
                            : 'w-2 bg-brand-border'
                      }`}
                    />
                  ))}
                </div>
                <span className="sr-only">{`Step ${currentStep + 1} of ${totalSteps}`}</span>
              </div>

              {/* Content */}
              <div className="mb-8 bg-brand-card border border-brand-border rounded-lg p-4">
                <p className="text-brand-muted text-sm leading-relaxed">
                  {steps[currentStep]?.content}
                </p>
              </div>

              {/* Actions */}
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  {currentStep > 0 ? (
                    <button
                      onClick={handlePrevious}
                      className="px-4 py-2 text-brand-muted hover:text-white transition-colors"
                    >
                      Previous
                    </button>
                  ) : (
                    <button
                      onClick={() => setRole(null)}
                      className="px-4 py-2 text-brand-muted hover:text-white transition-colors"
                    >
                      Back
                    </button>
                  )}
                  <button
                    onClick={handleSkip}
                    className="px-4 py-2 text-brand-muted hover:text-white transition-colors"
                  >
                    Skip
                  </button>
                </div>
                <button
                  onClick={handleNext}
                  className="px-6 py-2 bg-brand-gold text-brand-dark font-semibold rounded-lg hover:bg-brand-amber transition-colors"
                >
                  {currentStep === totalSteps - 1 ? 'Get Started' : 'Next'}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export function isFirstTimeUser(): boolean {
  if (typeof window === 'undefined') return false;
  return !localStorage.getItem(ONBOARDING_STORAGE_KEY);
}

export function resetOnboardingFlag(): void {
  if (typeof window === 'undefined') return;
  localStorage.removeItem(ONBOARDING_STORAGE_KEY);
}
