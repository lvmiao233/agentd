type StreamWriter<TChunk = unknown> = {
  write(chunk: TChunk): void;
};

export function consumeRunAgentStream<TChunk = unknown>(params: {
  responseBody: ReadableStream<Uint8Array>;
  textId: string;
  writer: StreamWriter<TChunk>;
}): Promise<{
  emitted: boolean;
  terminalReached: boolean;
  finishReason: 'stop' | 'error' | null;
}>;
