import { promises as fs } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

async function runSpec(specPath) {
  const absoluteSpecPath = path.resolve(specPath);
  const content = await fs.readFile(absoluteSpecPath, 'utf8');
  const compiledPath = absoluteSpecPath.replace(/\.ts$/, '.compiled.mjs');
  await fs.writeFile(compiledPath, content, 'utf8');

  try {
    const module = await import(pathToFileURL(compiledPath).href);
    if (typeof module.run !== 'function') {
      throw new Error(`Spec ${specPath} must export an async run() function`);
    }
    await module.run();
  } finally {
    await fs.rm(compiledPath, { force: true });
  }
}

async function main() {
  const rawArgs = process.argv.slice(2);
  const args = [];
  for (let i = 0; i < rawArgs.length; i += 1) {
    if (rawArgs[i] === '--filter') {
      i += 1;
      continue;
    }
    args.push(rawArgs[i]);
  }

  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const requested = args.length > 0 ? args : ['chat-page-streaming.spec.ts'];
  const specs = requested.map((spec) =>
    path.isAbsolute(spec) ? spec : path.resolve(scriptDir, spec)
  );

  for (const spec of specs) {
    await runSpec(spec);
    process.stdout.write(`PASS ${path.basename(spec)}\n`);
  }
}

main().catch((error) => {
  process.stderr.write(`${error.stack || error}\n`);
  process.exit(1);
});
