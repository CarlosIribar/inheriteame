export type NativeRequest = { protocolVersion: 1; projectDir: string; all?: boolean; dryRun?: boolean; check?: boolean; allowUnstaged?: boolean };
export type FileReport = { path: string; status: string; reason: string; diff: string | null };
export type NativeFinished = {
  protocolVersion: 1; event: 'finished'; exitCode: number;
  result: { schemaVersion: number; selectedFiles: number; reviewedFiles: number; changedFiles: number; excludedFiles: number; proposedEdits: number; errors: number; diagnostics: string[]; files: FileReport[] };
};
export function assertFinished(value: unknown): asserts value is NativeFinished {
  if (!value || typeof value !== 'object') throw new Error('Invalid native response.');
  const event = value as Partial<NativeFinished>;
  if (event.protocolVersion !== 1 || event.event !== 'finished' || ![0, 1, 2].includes(event.exitCode ?? -1) || !event.result || event.result.schemaVersion !== 1 || !Array.isArray(event.result.files) || !Array.isArray(event.result.diagnostics)) throw new Error('Incompatible native response.');
}
