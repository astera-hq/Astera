import { render, screen, fireEvent } from '@testing-library/react';
import GlossaryTerm from '../GlossaryTerm';
import { glossary } from '@/lib/glossary';

describe('GlossaryTerm', () => {
  const factoringFee = glossary.find((e) => e.id === 'factoring-fee')!;

  it('renders the term label and no tooltip by default', () => {
    render(<GlossaryTerm id="factoring-fee" />);
    expect(screen.getByRole('button', { name: /factoring fee: definition/i })).toBeInTheDocument();
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });

  it('shows the definition tooltip on focus', () => {
    render(<GlossaryTerm id="factoring-fee" />);
    const trigger = screen.getByRole('button', { name: /factoring fee: definition/i });
    fireEvent.focus(trigger);
    expect(screen.getByRole('tooltip')).toHaveTextContent(factoringFee.definition);
  });

  it('hides the tooltip on blur', () => {
    render(<GlossaryTerm id="factoring-fee" />);
    const trigger = screen.getByRole('button', { name: /factoring fee: definition/i });
    fireEvent.focus(trigger);
    expect(screen.getByRole('tooltip')).toBeInTheDocument();
    fireEvent.blur(trigger);
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });

  it('links the trigger to the tooltip via aria-describedby when visible', () => {
    render(<GlossaryTerm id="factoring-fee" />);
    const trigger = screen.getByRole('button', { name: /factoring fee: definition/i });
    const describedByWrapper = trigger.parentElement!;
    fireEvent.focus(trigger);
    const tooltip = screen.getByRole('tooltip');
    expect(describedByWrapper).toHaveAttribute('aria-describedby', tooltip.id);
  });

  it('renders custom children as the visible label while keeping the glossary definition', () => {
    render(<GlossaryTerm id="collateral-ratio">20%</GlossaryTerm>);
    expect(screen.getByRole('button', { name: /collateral ratio: definition/i })).toHaveTextContent(
      '20%',
    );
  });

  it('falls back to plain children when the id is unknown, without throwing', () => {
    render(<GlossaryTerm id="not-a-real-term">fallback text</GlossaryTerm>);
    expect(screen.getByText('fallback text')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
