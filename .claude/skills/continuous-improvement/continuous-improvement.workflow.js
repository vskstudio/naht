export const meta = {
    name: 'continuous-improvement',
    description: 'Audit one module across 8 axes (CI, architecture, performance, code quality, security, bugs, dead comments, doc drift) plus feature ideas; verify every finding adversarially; return a prioritized, deduped report split into safe-autofixes vs proposals.',
    phases: [
        { title: 'Scan', detail: 'one agent per axis sweeps the target module' },
        { title: 'Verify', detail: 'an adversarial judge confirms or refutes each finding' },
        { title: 'Synthesize', detail: 'dedup, prioritize, split autofix vs proposal' },
    ],
}

// args: { target: string (path relative to repo root, e.g. "core/src/modules/raffle"),
//         repoRoot?: string, conventions?: string }
const target = (args && args.target) || 'core/src/modules/raffle'
const conventions =
    (args && args.conventions) ||
    'Read CODING_CONVENTIONS.md at the repo root first — it is the source of truth. Key rules: HEXA/DDD/SOLID layering (domain=pure no-I/O, application=usecases, infrastructure=repositories, presentation=routes/commands; dependencies point inward). HARD RULE: never `export function` / `export const = () =>` — all exported behavior on classes as `public static`/instance methods; numeric consts → `public static readonly`; types/interfaces may stay free. No useless comments (explain WHY not WHAT; no commented-out code, no stale/redundant JSDoc). No `any` (use `unknown` + guards). CI gate (tsc+eslint+biome+vitest) must stay green. Biome 4-space indent, 100-col. Prisma via repository pattern + `$transaction` arrays.'

const FINDING_SCHEMA = {
    type: 'object',
    additionalProperties: false,
    properties: {
        findings: {
            type: 'array',
            items: {
                type: 'object',
                additionalProperties: false,
                properties: {
                    title: { type: 'string', description: 'One-line summary of the issue' },
                    file: { type: 'string', description: 'Path relative to repo root' },
                    line: { type: 'string', description: 'Line number or range, or "" if file-wide' },
                    severity: { type: 'string', enum: ['critical', 'high', 'medium', 'low'] },
                    effort: { type: 'string', enum: ['trivial', 'small', 'medium', 'large'] },
                    safeAutofix: {
                        type: 'boolean',
                        description:
                            'true ONLY for mechanical, low-risk fixes (delete dead comment, lint/format, doc text, obvious type tightening). false for anything touching behavior, architecture, or public API.',
                    },
                    detail: { type: 'string', description: 'What is wrong and why it matters' },
                    suggestion: { type: 'string', description: 'Concrete fix or change to make' },
                },
                required: ['title', 'file', 'line', 'severity', 'effort', 'safeAutofix', 'detail', 'suggestion'],
            },
        },
    },
    required: ['findings'],
}

const FEATURE_SCHEMA = {
    type: 'object',
    additionalProperties: false,
    properties: {
        features: {
            type: 'array',
            items: {
                type: 'object',
                additionalProperties: false,
                properties: {
                    title: { type: 'string' },
                    rationale: { type: 'string', description: 'Why it would be valuable, grounded in what this module already does' },
                    effort: { type: 'string', enum: ['small', 'medium', 'large'] },
                },
                required: ['title', 'rationale', 'effort'],
            },
        },
    },
    required: ['features'],
}

const VERDICT_SCHEMA = {
    type: 'object',
    additionalProperties: false,
    properties: {
        isReal: { type: 'boolean' },
        reason: { type: 'string', description: 'Why it is real or a false positive' },
        adjustedSeverity: { type: 'string', enum: ['critical', 'high', 'medium', 'low'] },
        safeAutofix: { type: 'boolean', description: 'Re-judged: is this truly safe to auto-apply?' },
    },
    required: ['isReal', 'reason', 'adjustedSeverity', 'safeAutofix'],
}

const AXES = [
    {
        key: 'ci',
        prompt: `Inspect every TypeScript/Rust file under "${target}" for things that would fail CI: type errors, \`any\` usage, eslint/biome violations, unused exports/imports, formatting that violates Biome 4-space/100-col. Report concrete violations with file:line.`,
    },
    {
        key: 'architecture',
        prompt: `Audit "${target}" for architecture/SOLID violations against these conventions:\n${conventions}\nFlag every free-function export, misplaced logic (I/O in domain/, business rules in repositories), god classes, leaky abstractions, and missing layer boundaries.`,
    },
    {
        key: 'performance',
        prompt: `Audit "${target}" for performance problems: N+1 queries, missing \`$transaction\` batching, unbounded loops/queries, redundant awaits in loops, sync work that should be batched, missing indexes implied by query shape. Report file:line with the hot path.`,
    },
    {
        key: 'quality',
        prompt: `Audit "${target}" for code-quality issues: duplication (DRY), overly complex functions, poor naming, missing error handling, swallowed errors, magic numbers, inconsistent patterns vs the rest of the module.`,
    },
    {
        key: 'security',
        prompt: `Audit "${target}" for security issues: missing input validation, injection (SQL/command), unsafe deserialization, secrets in code, missing authz checks, unsafe BigInt/number coercion, timing-unsafe comparisons, SSRF. Report file:line.`,
    },
    {
        key: 'bugs',
        prompt: `Hunt for real bugs in "${target}": off-by-one, wrong await/async, race conditions, unhandled rejections, incorrect null/undefined handling, wrong comparison operators, logic that contradicts the apparent intent. Report file:line with the failure scenario.`,
    },
    {
        key: 'comments',
        prompt: `Scan "${target}" for USELESS comments only: comments that restate the code, commented-out dead code, stale comments that no longer match the code, redundant JSDoc. Do NOT flag comments that explain WHY. Mark each as safeAutofix:true. Report file:line.`,
    },
    {
        key: 'docs',
        prompt: `Compare "${target}" against any docs that describe it (README, docs/, CLAUDE.md, design specs). Flag doc statements that no longer match the current code (drifted API, removed/renamed symbols, changed behavior). suggestion = the corrected doc text. Mark text-only doc edits as safeAutofix:true. Report file:line of the doc.`,
    },
]

phase('Scan')

// Findings: one finder per axis, each verified independently as it lands (pipeline, no barrier).
const verified = await pipeline(
    AXES,
    (axis) =>
        agent(axis.prompt, {
            label: `scan:${axis.key}`,
            phase: 'Scan',
            agentType: 'Explore',
            schema: FINDING_SCHEMA,
        }),
    (scan, axis) =>
        parallel(
            (scan?.findings || []).map((f) => () =>
                agent(
                    `Adversarially verify this ${axis.key} finding in the ganyu codebase. Read the actual code at ${f.file}:${f.line}. Default to isReal:false if you cannot confirm it from the real code. Finding: ${JSON.stringify(f)}`,
                    { label: `verify:${axis.key}`, phase: 'Verify', agentType: 'Explore', schema: VERDICT_SCHEMA },
                ).then((v) => ({ ...f, axis: axis.key, verdict: v })),
            ),
        ),
)

// Feature proposals run alongside; not verified (they are ideas, not claims).
phase('Synthesize')
const featureResult = await agent(
    `Propose genuinely useful features for the module at "${target}", grounded in what it already does and how it fits the wider Discord bot game ecosystem. Avoid generic ideas; each must build on existing code. Read the module first.`,
    { label: 'features', phase: 'Synthesize', agentType: 'Explore', schema: FEATURE_SCHEMA },
)

const confirmed = verified
    .flat()
    .filter(Boolean)
    .filter((f) => f.verdict && f.verdict.isReal)
    .map((f) => ({
        ...f,
        severity: f.verdict.adjustedSeverity || f.severity,
        safeAutofix: f.safeAutofix && f.verdict.safeAutofix,
    }))

// Dedup by file+title (same issue surfaced by two axes).
const seen = new Set()
const deduped = confirmed.filter((f) => {
    const k = `${f.file}::${f.title.toLowerCase().slice(0, 40)}`
    if (seen.has(k)) return false
    seen.add(k)
    return true
})

const sevRank = { critical: 0, high: 1, medium: 2, low: 3 }
deduped.sort((a, b) => (sevRank[a.severity] - sevRank[b.severity]) || a.file.localeCompare(b.file))

const safeFixes = deduped.filter((f) => f.safeAutofix)
const proposals = deduped.filter((f) => !f.safeAutofix)

log(`${target}: ${deduped.length} confirmed findings (${safeFixes.length} safe-autofix, ${proposals.length} proposals), ${(featureResult?.features || []).length} feature ideas`)

return {
    target,
    counts: {
        confirmed: deduped.length,
        safeFixes: safeFixes.length,
        proposals: proposals.length,
        features: (featureResult?.features || []).length,
    },
    safeFixes,
    proposals,
    features: featureResult?.features || [],
}
