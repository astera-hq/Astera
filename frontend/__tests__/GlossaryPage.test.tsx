import { render, screen } from '@testing-library/react';
import GlossaryPage from '@/app/glossary/page';
import { glossary } from '@/lib/glossary';

describe('GlossaryPage', () => {
  it('renders every glossary term and definition', async () => {
    render(await GlossaryPage());
    for (const entry of glossary) {
      expect(screen.getByText(entry.term)).toBeInTheDocument();
      expect(screen.getByText(entry.definition)).toBeInTheDocument();
    }
  });

  it('gives each entry an id matching its glossary id for #anchor linking', async () => {
    const { container } = render(await GlossaryPage());
    for (const entry of glossary) {
      expect(container.querySelector(`#${entry.id}`)).not.toBeNull();
    }
  });
});
