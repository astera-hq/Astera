/**
 * Structured logging for the indexer. Replaces ad hoc console.log/error
 * calls so log output can be shipped to and queried by whatever log
 * aggregation the deployment already uses.
 */

import pino from "pino";

export const logger = pino({
  name: "astera-indexer",
  level: process.env.LOG_LEVEL || "info",
  timestamp: pino.stdTimeFunctions.isoTime,
});
