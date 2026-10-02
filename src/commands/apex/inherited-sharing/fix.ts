import { Flags } from '@oclif/core';
import { SfCommand } from '@salesforce/sf-plugins-core';
import { performance } from 'node:perf_hooks';
import { runNative } from '../../../native/run.js';

export default class Fix extends SfCommand<unknown> {
  public static readonly summary = 'Add inherited sharing to Apex classes without explicit sharing.';
  public static readonly requiresProject = false;
  public static readonly enableJsonFlag = true;
  public static readonly flags = {
    all: Flags.boolean({ summary: 'Process all .cls files in package directories.' }),
    'project-dir': Flags.directory({ summary: 'Salesforce project directory or a directory inside it.', default: process.cwd() }),
    'dry-run': Flags.boolean({ summary: 'Show proposed diffs without writing files.' }),
    check: Flags.boolean({ summary: 'Check without writing; exit 1 if corrections are needed.' }),
    'allow-unstaged': Flags.boolean({ summary: 'Allow staged files with working-tree changes.' }),
    quiet: Flags.boolean({ summary: 'Suppress non-error human output.' }),
    verbose: Flags.boolean({ summary: 'Explain the decision for every selected file.' }),
  };
  public async run(): Promise<unknown> {
    const startedAt = performance.now();
    const { flags } = await this.parse(Fix);
    if (flags.check && flags['dry-run']) this.error('--check cannot be used with --dry-run.', { exit: 2 });
    if (flags.quiet && flags.verbose) this.error('--quiet cannot be used with --verbose.', { exit: 2 });
    if (flags.all && flags['allow-unstaged']) this.error('--allow-unstaged cannot be used with --all.', { exit: 2 });
    let finished;
    try {
      finished = await runNative({ protocolVersion: 1, projectDir: flags['project-dir'], all: flags.all, dryRun: flags['dry-run'], check: flags.check, allowUnstaged: flags['allow-unstaged'] });
    } catch (error) {
      this.error(error instanceof Error ? error.message : String(error), { exit: 2 });
    }
    const result = { ...finished.result, durationSeconds: (performance.now() - startedAt) / 1000 };
    if (!this.jsonEnabled()) {
      if (!flags.quiet) {
        for (const file of result.files) {
          if (flags.verbose) this.log(`${file.path}: ${file.status} (${file.reason})`);
          if (flags['dry-run'] && file.diff) this.log(file.diff);
        }
        this.log(`inheriteame: ${result.reviewedFiles} reviewed, ${result.changedFiles} modified, ${result.excludedFiles} excluded, ${result.proposedEdits} proposed edits, ${result.errors} errors in ${result.durationSeconds.toFixed(3)} seconds.`);
      }
      for (const diagnostic of result.diagnostics) this.warn(diagnostic);
    }
    if (finished.exitCode !== 0) process.exitCode = finished.exitCode;
    return { result };
  }
}
