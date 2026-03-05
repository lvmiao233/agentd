type StreamWriter<TChunk = unknown> = {
  write(chunk: TChunk): void;
};

export function emitRunAgentStreamLine<TChunk = unknown>(params: {
  lineRaw: string;
  textId: string;
  writer: StreamWriter<TChunk>;
}): {
  emitted: boolean;
  terminalReached: boolean;
};
