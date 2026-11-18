import type { SmeTrajectory } from './replay';

export interface PredictiveQualityReport {
  cohortSize: { defaulted: number; nonDefaulted: number };
  meanScoreBeforeOutcome: { defaulted: number | null; nonDefaulted: number | null };
  /**
   * Rank-based separation (a Mann-Whitney U / AUC equivalent): the fraction
   * of (non-defaulted, defaulted) score pairs where the non-defaulted SME's
   * score is strictly higher. 1.0 = perfect separation, 0.5 = no better than
   * chance, <0.5 = scores are backwards (higher score, more likely default).
   */
  separationAuc: number | null;
}

function mean(values: number[]): number | null {
  if (values.length === 0) return null;
  return values.reduce((a, b) => a + b, 0) / values.length;
}

/**
 * Compute how well `scoreBeforeOutcome` predicts `everDefaulted` across a set
 * of SME trajectories. Requires at least one SME in each cohort to compute
 * `separationAuc`; otherwise that field is `null`.
 */
export function computePredictiveQuality(trajectories: SmeTrajectory[]): PredictiveQualityReport {
  const defaultedScores = trajectories
    .filter((t) => t.everDefaulted && t.scoreBeforeOutcome !== null)
    .map((t) => t.scoreBeforeOutcome as number);
  const nonDefaultedScores = trajectories
    .filter((t) => !t.everDefaulted && t.scoreBeforeOutcome !== null)
    .map((t) => t.scoreBeforeOutcome as number);

  let separationAuc: number | null = null;
  if (defaultedScores.length > 0 && nonDefaultedScores.length > 0) {
    let wins = 0;
    let ties = 0;
    for (const nd of nonDefaultedScores) {
      for (const d of defaultedScores) {
        if (nd > d) wins += 1;
        else if (nd === d) ties += 1;
      }
    }
    const totalPairs = nonDefaultedScores.length * defaultedScores.length;
    separationAuc = (wins + 0.5 * ties) / totalPairs;
  }

  return {
    cohortSize: { defaulted: defaultedScores.length, nonDefaulted: nonDefaultedScores.length },
    meanScoreBeforeOutcome: {
      defaulted: mean(defaultedScores),
      nonDefaulted: mean(nonDefaultedScores),
    },
    separationAuc,
  };
}
