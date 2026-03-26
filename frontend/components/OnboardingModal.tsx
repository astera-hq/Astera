'use client';

import { useState, useEffect, useRef } from 'react';

interface OnboardingModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const ONBOARDING_STORAGE_KEY = 'astera-onboarding-completed';

export default function OnboardingModal({ isOpen, onClose }: OnboardingModalProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const modalRef = useRef<HTMLDivElement>(null);
  const firstFocusableRef = useRef<HTMLButtonElement>(null);

  const steps = [
    {
      title: 'Welcome to Astera',
      content: (
        <div className="space-y-4">
          <p className="text-brand-muted">
            Astera is a decentralized invoice financing platform that helps SMEs access immediate
            liquidity by tokenizing their invoices on the Stellar blockchain.
          </p>
          <div className="bg-brand-card border border-brand-border rounded-lg p-4">
            <h4 className="font-semibold mb-2">How it works:</h4>
            <ul className="space-y-2 text-sm text-brand-muted">
              <li>• Create invoices with your debtor information</li>
              <li>• Tokenize them on the Stellar blockchain</li>
              <li>• Get funded by investors or use co-funding</li>
              <li>• Repay when your customer pays the invoice</li>
            </ul>
          </div>
        </div>
      ),
    },
    {
      title: 'Getting Testnet USDC',
      content: (
        <div className="space-y-4">
          <p className="text-brand-muted">
            To test the platform, you&apos;ll need some testnet USDC in your wallet.
          </p>
          <div className="bg-brand-card border border-brand-border rounded-lg p-4">
            <h4 className="font-semibold mb-2">Steps to get testnet USDC:</h4>
            <ol className="space-y-2 text-sm text-brand-muted list-decimal list-inside">
              <li>• Connect your Freighter wallet</li>
              <li>• Visit the Stellar Testnet Faucet</li>
              <li>• Enter your wallet address</li>
              <li>• Request testnet USDC (10 USDC per request)</li>
              <li>• Wait for the tokens to arrive in your wallet</li>
            </ol>
          </div>
          <div className="bg-blue-900/20 border border-blue-800/50 rounded-lg p-3">
            <p className="text-sm text-blue-400">
              <strong>Note:</strong> Testnet USDC has no real value and is only for testing
              purposes.
            </p>
          </div>
        </div>
      ),
    },
    {
      title: 'Creating Your First Invoice',
      content: (
        <div className="space-y-4">
          <p className="text-brand-muted">
            Ready to create your first invoice? Here&apos;s what you&apos;ll need:
          </p>
          <div className="bg-brand-card border border-brand-border rounded-lg p-4">
            <h4 className="font-semibold mb-2">Required Information:</h4>
            <ul className="space-y-2 text-sm text-brand-muted">
              <li>• Debtor&apos;s wallet address (who owes you)</li>
              <li>• Invoice amount in USDC</li>
              <li>• Due date (when payment is expected)</li>
              <li>• Description of goods/services</li>
            </ul>
          </div>
          <div className="bg-green-900/20 border border-green-800/50 rounded-lg p-3">
            <p className="text-sm text-green-400">
              <strong>Pro tip:</strong> Start with a small amount to test the process before
              creating larger invoices.
            </p>
          </div>
        </div>
      ),
    },
  ];

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
    if (currentStep < steps.length - 1) {
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
              {steps[currentStep].title}
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

          {/* Progress indicator */}
          <div className="flex items-center justify-center mb-6">
            <div className="flex items-center space-x-2">
              {steps.map((_, index) => (
                <div
                  key={index}
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
          </div>

          {/* Content */}
          <div className="mb-8">{steps[currentStep].content}</div>

          {/* Actions */}
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-3">
              {currentStep > 0 && (
                <button
                  onClick={handlePrevious}
                  className="px-4 py-2 text-brand-muted hover:text-white transition-colors"
                >
                  Previous
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
              {currentStep === steps.length - 1 ? 'Get Started' : 'Next'}
            </button>
          </div>
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
