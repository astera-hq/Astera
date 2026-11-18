import type { SmeTrajectory } from './replay';
import type { PredictiveQualityReport } from './metrics';

/** Render a human-readable Markdown report a reviewer can read to judge predictive quality. */
export function renderReport(
  trajectories: SmeTrajectory[],
  quality: PredictiveQualityReport,
): string {
  const lines: string[] = [];
  lines.push('# Credit Score Backtest Report');
  lines.push('');
  lines.push(
    `Replayed ${trajectories.length} SME(s) from the indexed \`credit_score\` payment/default event stream.`,
  );
  lines.push('');
  lines.push('## Predictive quality');
  lines.push('');
  lines.push(
    `- Defaulted cohort: ${quality.cohortSize.defaulted} SME(s), mean score before default: ${
      quality.meanScoreBeforeOutcome.defaulted?.toFixed(1) ?? 'n/a'
    }`,
  );
  lines.push(
    `- Non-defaulted cohort: ${quality.cohortSize.nonDefaulted} SME(s), mean score: ${
      quality.meanScoreBeforeOutcome.nonDefaulted?.toFixed(1) ?? 'n/a'
    }`,
  );
  if (quality.separationAuc !== null) {
    lines.push(
      `- Separation (AUC-equivalent): **${quality.separationAuc.toFixed(3)}** ` +
        `(1.0 = perfect separation, 0.5 = no better than chance, <0.5 = inverted)`,
    );
  } else {
    lines.push(
      '- Separation: not computable — need at least one SME in each cohort (defaulted and non-defaulted).',
    );
  }
  lines.push('');
  lines.push('## Per-SME trajectories');
  lines.push('');
  lines.push('| SME | Samples | Ever defaulted | Score before outcome |');
  lines.push('|---|---|---|---|');
  for (const t of trajectories) {
    lines.push(
      `| ${t.sme} | ${t.samples.length} | ${t.everDefaulted ? 'yes' : 'no'} | ${
        t.scoreBeforeOutcome ?? 'n/a'
      } |`,
    );
  }
  lines.push('');
  return lines.join('\n');
}
