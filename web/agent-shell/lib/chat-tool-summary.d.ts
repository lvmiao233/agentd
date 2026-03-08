export type ToolOutputFact = {
  label: string;
  value: string;
};

export function summarizeToolInput(input: unknown): string;
export function summarizeToolOutput(output: unknown, errorText: unknown): string;
export function buildToolOutputFacts(output: unknown, errorText: unknown): ToolOutputFact[];
