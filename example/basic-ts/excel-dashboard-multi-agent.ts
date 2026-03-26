/**
 * excel-dashboard-multi-agent.ts
 *
 * Multi-agent Excel dashboard builder using @getenki/ai.
 *
 * Five specialist agents collaborate via NativeMultiAgentRuntime:
 *   1. Requirements Agent  – parses natural-language dashboard requirements
 *   2. Planner Agent       – creates a structured dashboard layout plan
 *   3. Review Agent        – validates the plan for completeness
 *   4. Excel Agent         – analyses an input workbook and builds the dashboard
 *   5. Orchestrator        – top-level coordinator that delegates to the others
 *
 * The workbook layer uses the `xlsx` package (SheetJS) to read source data
 * and write a dashboard-oriented workbook with KPI rows, chart-ready
 * aggregated sections, and planning notes.
 *
 * Usage:
 *   npx tsx excel-dashboard-multi-agent.ts \
 *     --requirements "Create a sales dashboard with revenue KPIs, region comparison, and top products" \
 *     --input sales_data.xlsx \
 *     --output sales_dashboard.xlsx
 */

import {
    JsAgentStatus,
    NativeEnkiAgent,
    NativeMultiAgentRuntime,
    type JsAgentCard,
    type JsMultiAgentMember,
} from '@getenki/ai'

import XLSX from 'xlsx'
import * as fs from 'node:fs'
import * as path from 'node:path'

declare const process: {
    cwd(): string
    env: Record<string, string | undefined>
    argv: string[]
    exitCode?: number
}

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

interface CliArgs {
    requirements: string
    inputPath: string
    outputPath: string
}

function parseArgs(): CliArgs {
    const argv = process.argv.slice(2)
    let requirements = 'Create a sales dashboard with revenue KPIs, region comparison, and top products'
    let inputPath = ''
    let outputPath = 'dashboard_output.xlsx'

    for (let i = 0; i < argv.length; i++) {
        switch (argv[i]) {
            case '--requirements':
                requirements = argv[++i] ?? requirements
                break
            case '--input':
                inputPath = argv[++i] ?? ''
                break
            case '--output':
                outputPath = argv[++i] ?? outputPath
                break
        }
    }

    return { requirements, inputPath, outputPath }
}

// ---------------------------------------------------------------------------
// Excel helpers
// ---------------------------------------------------------------------------

interface SheetSummary {
    name: string
    rowCount: number
    columns: string[]
    sampleRows: Record<string, unknown>[]
    numericColumns: string[]
}

function summariseWorkbook(filePath: string): SheetSummary[] {
    const wb = XLSX.readFile(filePath)
    const summaries: SheetSummary[] = []

    for (const name of wb.SheetNames) {
        const ws = wb.Sheets[name]
        if (!ws) continue

        const rows = XLSX.utils.sheet_to_json<Record<string, unknown>>(ws)
        const columns = rows.length > 0 ? Object.keys(rows[0] as object) : []

        // Detect numeric columns by sampling the first 20 rows
        const numericColumns = columns.filter((col) =>
            rows
                .slice(0, 20)
                .some((row) => typeof row[col] === 'number'),
        )

        summaries.push({
            name,
            rowCount: rows.length,
            columns,
            sampleRows: rows.slice(0, 5),
            numericColumns,
        })
    }

    return summaries
}

interface KpiRow {
    label: string
    value: number | string
    format?: string
}

interface AggregateSection {
    title: string
    headers: string[]
    rows: (string | number)[][]
}

interface DashboardData {
    kpis: KpiRow[]
    sections: AggregateSection[]
    planningNotes: string[]
}

function buildDashboardWorkbook(
    sourcePath: string | null,
    dashboard: DashboardData,
    outputPath: string,
): void {
    const wb = XLSX.utils.book_new()

    // --- Dashboard sheet ---------------------------------------------------
    const dashRows: (string | number)[][] = []

    // KPI header
    dashRows.push(['KEY PERFORMANCE INDICATORS'])
    dashRows.push(['Metric', 'Value'])
    for (const kpi of dashboard.kpis) {
        dashRows.push([kpi.label, kpi.value as string | number])
    }
    dashRows.push([]) // spacer

    // Aggregated sections (chart-ready)
    for (const section of dashboard.sections) {
        dashRows.push([section.title])
        dashRows.push(section.headers)
        for (const row of section.rows) {
            dashRows.push(row)
        }
        dashRows.push([]) // spacer
    }

    // Planning notes
    if (dashboard.planningNotes.length > 0) {
        dashRows.push(['PLANNING NOTES'])
        for (const note of dashboard.planningNotes) {
            dashRows.push([note])
        }
    }

    const dashWs = XLSX.utils.aoa_to_sheet(dashRows)

    // Auto-size columns
    const maxColWidths: number[] = []
    for (const row of dashRows) {
        for (let c = 0; c < row.length; c++) {
            const len = String(row[c] ?? '').length
            maxColWidths[c] = Math.max(maxColWidths[c] ?? 0, len)
        }
    }
    dashWs['!cols'] = maxColWidths.map((w) => ({ wch: Math.min(w + 2, 60) }))

    XLSX.utils.book_append_sheet(wb, dashWs, 'Dashboard')

    // --- Copy source data sheets ------------------------------------------
    if (sourcePath && fs.existsSync(sourcePath)) {
        const sourceWb = XLSX.readFile(sourcePath)
        for (const name of sourceWb.SheetNames) {
            const srcWs = sourceWb.Sheets[name]
            if (srcWs) {
                XLSX.utils.book_append_sheet(wb, srcWs, `Source_${name}`.slice(0, 31))
            }
        }
    }

    XLSX.writeFile(wb, outputPath)
}

// ---------------------------------------------------------------------------
// Shared state passed between agents via tool closures
// ---------------------------------------------------------------------------

interface AgentContext {
    requirements: string
    inputPath: string
    outputPath: string
    sheetSummaries: SheetSummary[]
    parsedRequirements: string
    dashboardPlan: string
    reviewFeedback: string
    dashboardData: DashboardData
}

// ---------------------------------------------------------------------------
// Agent definitions
// ---------------------------------------------------------------------------

function createAgents(ctx: AgentContext, model: string, workspaceHome: string) {
    // ------- Requirements Agent -------------------------------------------
    const requirementsAgent = NativeEnkiAgent.withTools(
        'RequirementsAgent',
        [
            'You are a requirements analyst for Excel dashboard projects.',
            'When asked, use the parse_requirements tool to extract structured requirements from natural language.',
            'Return only the parsed output.',
        ].join(' '),
        model,
        10,
        workspaceHome,
        [
            {
                id: 'parse_requirements',
                description:
                    'Parse natural-language dashboard requirements into a structured list of KPIs, chart types, and data dimensions.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        text: { type: 'string', description: 'Raw requirements text' },
                    },
                    required: ['text'],
                },
                execute(inputJson: string): string {
                    const args = JSON.parse(inputJson) as { text: string }
                    const text = args.text.toLowerCase()

                    const kpis: string[] = []
                    const charts: string[] = []
                    const dimensions: string[] = []

                    // Extract KPI requests
                    const kpiPatterns = ['revenue', 'sales', 'profit', 'growth', 'margin', 'count', 'total', 'average']
                    for (const p of kpiPatterns) {
                        if (text.includes(p)) kpis.push(p)
                    }

                    // Extract chart types
                    const chartPatterns = ['comparison', 'trend', 'distribution', 'top', 'breakdown', 'pie', 'bar', 'line']
                    for (const p of chartPatterns) {
                        if (text.includes(p)) charts.push(p)
                    }

                    // Extract dimensions
                    const dimPatterns = ['region', 'product', 'category', 'date', 'month', 'quarter', 'year', 'customer']
                    for (const p of dimPatterns) {
                        if (text.includes(p)) dimensions.push(p)
                    }

                    // Fallback if nothing matched
                    if (kpis.length === 0) kpis.push('total', 'average')
                    if (charts.length === 0) charts.push('bar', 'comparison')

                    const result = {
                        kpis,
                        chartTypes: charts,
                        dimensions,
                        rawRequirements: args.text,
                    }
                    ctx.parsedRequirements = JSON.stringify(result, null, 2)
                    return ctx.parsedRequirements
                },
            },
        ],
        null,
    )

    // ------- Planner Agent ------------------------------------------------
    const plannerAgent = NativeEnkiAgent.withTools(
        'PlannerAgent',
        [
            'You are a dashboard layout planner.',
            'Use the create_dashboard_plan tool to generate a structured plan.',
            'Combine the parsed requirements with available data columns.',
            'Return only the plan JSON.',
        ].join(' '),
        model,
        10,
        workspaceHome,
        [
            {
                id: 'create_dashboard_plan',
                description:
                    'Generate a dashboard layout plan given parsed requirements and available data columns.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        requirements: { type: 'string', description: 'Parsed requirements JSON' },
                        availableColumns: {
                            type: 'string',
                            description: 'JSON array of available data column names',
                        },
                    },
                    required: ['requirements'],
                },
                execute(inputJson: string): string {
                    const args = JSON.parse(inputJson) as {
                        requirements: string
                        availableColumns?: string
                    }

                    let reqs: { kpis: string[]; chartTypes: string[]; dimensions: string[] }
                    try {
                        reqs = JSON.parse(args.requirements)
                    } catch {
                        reqs = { kpis: ['total'], chartTypes: ['bar'], dimensions: [] }
                    }

                    const numericCols = ctx.sheetSummaries.flatMap((s) => s.numericColumns)

                    // Build KPI definitions
                    const kpiDefs = reqs.kpis.map((kpi) => {
                        const col = numericCols.find((c) => c.toLowerCase().includes(kpi)) ?? numericCols[0] ?? 'value'
                        return { label: `${kpi.charAt(0).toUpperCase() + kpi.slice(1)}`, sourceColumn: col, aggregation: 'sum' }
                    })

                    // Build chart sections
                    const chartSections = reqs.chartTypes.map((chartType) => {
                        const groupCol =
                            reqs.dimensions.find((d) =>
                                ctx.sheetSummaries.some((s) =>
                                    s.columns.some((c) => c.toLowerCase().includes(d)),
                                ),
                            ) ?? ctx.sheetSummaries[0]?.columns[0] ?? 'category'

                        const valueCol = numericCols[0] ?? 'value'
                        return {
                            chartType,
                            title: `${chartType.charAt(0).toUpperCase() + chartType.slice(1)} Chart`,
                            groupByColumn: groupCol,
                            valueColumn: valueCol,
                            aggregation: 'sum',
                        }
                    })

                    const plan = { kpis: kpiDefs, charts: chartSections, sourceSheets: ctx.sheetSummaries.map((s) => s.name) }
                    ctx.dashboardPlan = JSON.stringify(plan, null, 2)
                    return ctx.dashboardPlan
                },
            },
        ],
        null,
    )

    // ------- Review Agent -------------------------------------------------
    const reviewAgent = NativeEnkiAgent.withTools(
        'ReviewAgent',
        [
            'You are a dashboard plan reviewer.',
            'Use the review_plan tool to validate the dashboard plan.',
            'Report any issues or confirm readiness.',
        ].join(' '),
        model,
        10,
        workspaceHome,
        [
            {
                id: 'review_plan',
                description: 'Validate a dashboard plan for completeness and data alignment.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        plan: { type: 'string', description: 'Dashboard plan JSON to review' },
                    },
                    required: ['plan'],
                },
                execute(inputJson: string): string {
                    const args = JSON.parse(inputJson) as { plan: string }
                    const issues: string[] = []
                    const availableCols = new Set(ctx.sheetSummaries.flatMap((s) => s.columns.map((c) => c.toLowerCase())))

                    let plan: { kpis?: { sourceColumn?: string }[]; charts?: { groupByColumn?: string; valueColumn?: string }[] }
                    try {
                        plan = JSON.parse(args.plan)
                    } catch {
                        return JSON.stringify({ approved: false, issues: ['Plan JSON is malformed.'] })
                    }

                    // Check KPI source columns exist
                    for (const kpi of plan.kpis ?? []) {
                        if (kpi.sourceColumn && !availableCols.has(kpi.sourceColumn.toLowerCase())) {
                            issues.push(`KPI source column "${kpi.sourceColumn}" not found in data.`)
                        }
                    }

                    // Check chart columns exist
                    for (const chart of plan.charts ?? []) {
                        if (chart.groupByColumn && !availableCols.has(chart.groupByColumn.toLowerCase())) {
                            issues.push(`Chart group column "${chart.groupByColumn}" not found in data.`)
                        }
                        if (chart.valueColumn && !availableCols.has(chart.valueColumn.toLowerCase())) {
                            issues.push(`Chart value column "${chart.valueColumn}" not found in data.`)
                        }
                    }

                    const approved = issues.length === 0
                    const result = { approved, issues, notes: approved ? 'Plan looks good – proceed with build.' : 'Fix the issues above before building.' }
                    ctx.reviewFeedback = JSON.stringify(result, null, 2)
                    return ctx.reviewFeedback
                },
            },
        ],
        null,
    )

    // ------- Excel Agent (analyser + builder) -----------------------------
    const excelAgent = NativeEnkiAgent.withTools(
        'ExcelAgent',
        [
            'You are an Excel dashboard builder.',
            'Use analyse_workbook to inspect the source data.',
            'Use build_dashboard to create the output workbook.',
            'Return the build summary.',
        ].join(' '),
        model,
        10,
        workspaceHome,
        [
            {
                id: 'analyse_workbook',
                description: 'Analyse an Excel workbook and return sheet summaries with column types.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        filePath: { type: 'string', description: 'Path to the input .xlsx file' },
                    },
                    required: ['filePath'],
                },
                execute(inputJson: string): string {
                    const args = JSON.parse(inputJson) as { filePath: string }
                    const p = args.filePath || ctx.inputPath

                    if (!p || !fs.existsSync(p)) {
                        return JSON.stringify({ error: `File not found: ${p}`, usingDemoData: true })
                    }

                    ctx.sheetSummaries = summariseWorkbook(p)
                    return JSON.stringify(ctx.sheetSummaries, null, 2)
                },
            },
            {
                id: 'build_dashboard',
                description:
                    'Build the dashboard workbook from the plan and write it to disk.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        plan: { type: 'string', description: 'Dashboard plan JSON' },
                        outputPath: { type: 'string', description: 'Output file path' },
                    },
                    required: ['plan'],
                },
                execute(inputJson: string): string {
                    const args = JSON.parse(inputJson) as { plan?: string; outputPath?: string }
                    const outPath = args.outputPath ?? ctx.outputPath

                    let plan: {
                        kpis?: { label: string; sourceColumn: string; aggregation: string }[]
                        charts?: { title: string; groupByColumn: string; valueColumn: string; aggregation: string }[]
                    }
                    try {
                        plan = JSON.parse(args.plan ?? ctx.dashboardPlan)
                    } catch {
                        plan = { kpis: [], charts: [] }
                    }

                    // Collect all source rows
                    const allRows: Record<string, unknown>[] = ctx.sheetSummaries.flatMap((s) => {
                        if (!ctx.inputPath || !fs.existsSync(ctx.inputPath)) return s.sampleRows
                        const wb = XLSX.readFile(ctx.inputPath)
                        const ws = wb.Sheets[s.name]
                        return ws ? XLSX.utils.sheet_to_json<Record<string, unknown>>(ws) : s.sampleRows
                    })

                    // Compute KPIs
                    const kpis: KpiRow[] = (plan.kpis ?? []).map((k) => {
                        const values = allRows
                            .map((r) => Number(r[k.sourceColumn]))
                            .filter((v) => !isNaN(v))

                        let value: number
                        switch (k.aggregation) {
                            case 'average':
                                value = values.length > 0 ? values.reduce((a, b) => a + b, 0) / values.length : 0
                                break
                            case 'count':
                                value = values.length
                                break
                            case 'min':
                                value = values.length > 0 ? Math.min(...values) : 0
                                break
                            case 'max':
                                value = values.length > 0 ? Math.max(...values) : 0
                                break
                            default: // sum
                                value = values.reduce((a, b) => a + b, 0)
                        }
                        return { label: k.label, value: Math.round(value * 100) / 100 }
                    })

                    // Build chart-ready aggregate sections
                    const sections: AggregateSection[] = (plan.charts ?? []).map((chart) => {
                        const grouped = new Map<string, number[]>()
                        for (const row of allRows) {
                            const groupKey = String(row[chart.groupByColumn] ?? 'Unknown')
                            const val = Number(row[chart.valueColumn])
                            if (!isNaN(val)) {
                                const arr = grouped.get(groupKey)
                                if (arr) {
                                    arr.push(val)
                                } else {
                                    grouped.set(groupKey, [val])
                                }
                            }
                        }

                        const headers = [chart.groupByColumn, `${chart.valueColumn} (${chart.aggregation})`]
                        const rows: (string | number)[][] = []
                        for (const [group, values] of grouped.entries()) {
                            const agg =
                                chart.aggregation === 'average'
                                    ? Math.round((values.reduce((a, b) => a + b, 0) / values.length) * 100) / 100
                                    : values.reduce((a, b) => a + b, 0)
                            rows.push([group, agg])
                        }

                        // Sort descending by value
                        rows.sort((a, b) => Number(b[1]) - Number(a[1]))

                        return { title: chart.title, headers, rows }
                    })

                    const planningNotes = [
                        `Generated from: ${ctx.inputPath || 'demo data'}`,
                        `Requirements: ${ctx.requirements}`,
                        `Plan review: ${ctx.reviewFeedback || 'N/A'}`,
                    ]

                    const dashboard: DashboardData = { kpis, sections, planningNotes }
                    ctx.dashboardData = dashboard

                    buildDashboardWorkbook(ctx.inputPath || null, dashboard, outPath)

                    return JSON.stringify({
                        success: true,
                        outputPath: outPath,
                        kpiCount: kpis.length,
                        sectionCount: sections.length,
                        message: `Dashboard written to ${outPath}`,
                    })
                },
            },
        ],
        null,
    )

    return { requirementsAgent, plannerAgent, reviewAgent, excelAgent }
}

// ---------------------------------------------------------------------------
// Multi-agent orchestrator members
// ---------------------------------------------------------------------------

function createOrchestratorMembers(model: string): JsMultiAgentMember[] {
    return [
        {
            agentId: 'orchestrator',
            name: 'Orchestrator',
            systemPromptPreamble: [
                'You are the orchestrator for an Excel dashboard builder pipeline.',
                'Use discover_agents to find specialist agents.',
                'Delegate tasks in this order:',
                '1. Delegate to ExcelAgent to analyse the workbook.',
                '2. Delegate to RequirementsAgent to parse the requirements.',
                '3. Delegate to PlannerAgent to create a dashboard plan.',
                '4. Delegate to ReviewAgent to validate the plan.',
                '5. Delegate to ExcelAgent to build the dashboard.',
                'Return a summary of the pipeline results.',
            ].join(' '),
            model,
            maxIterations: 30,
            capabilities: ['orchestration', 'planning'],
        },
        {
            agentId: 'requirements-agent',
            name: 'RequirementsAgent',
            systemPromptPreamble:
                'You are a requirements analyst. Parse dashboard requirements when delegated to you.',
            model,
            maxIterations: 10,
            capabilities: ['requirements', 'analysis'],
        },
        {
            agentId: 'planner-agent',
            name: 'PlannerAgent',
            systemPromptPreamble:
                'You are a dashboard planner. Create layout plans when delegated to you.',
            model,
            maxIterations: 10,
            capabilities: ['planning', 'layout'],
        },
        {
            agentId: 'review-agent',
            name: 'ReviewAgent',
            systemPromptPreamble:
                'You are a plan reviewer. Validate dashboard plans when delegated to you.',
            model,
            maxIterations: 10,
            capabilities: ['review', 'validation'],
        },
        {
            agentId: 'excel-agent',
            name: 'ExcelAgent',
            systemPromptPreamble:
                'You are an Excel specialist. Analyse workbooks and build dashboards when delegated to you.',
            model,
            maxIterations: 10,
            capabilities: ['excel', 'data-analysis', 'dashboard-building'],
        },
    ]
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
    const args = parseArgs()
    const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
    const workspaceHome = process.cwd()

    console.log('╔════════════════════════════════════════════════════════════╗')
    console.log('║       Excel Dashboard Multi-Agent Builder (TypeScript)    ║')
    console.log('╚════════════════════════════════════════════════════════════╝')
    console.log()
    console.log(`  Model:        ${model}`)
    console.log(`  Input:        ${args.inputPath || '(no input file – using demo data)'}`)
    console.log(`  Output:       ${args.outputPath}`)
    console.log(`  Requirements: ${args.requirements}`)
    console.log()

    // Shared mutable context across agents
    const ctx: AgentContext = {
        requirements: args.requirements,
        inputPath: args.inputPath ? path.resolve(args.inputPath) : '',
        outputPath: args.outputPath,
        sheetSummaries: [],
        parsedRequirements: '',
        dashboardPlan: '',
        reviewFeedback: '',
        dashboardData: { kpis: [], sections: [], planningNotes: [] },
    }

    // Create specialist agents (with tools wired to ctx)
    const { requirementsAgent, plannerAgent, reviewAgent, excelAgent } = createAgents(ctx, model, workspaceHome)

    // --- Standalone agent pipeline (sequential delegation) ----------------
    // This shows the NativeEnkiAgent-based approach: each agent is invoked
    // individually, and results flow through the shared context.

    console.log('─── Stage 1: Analyse input workbook ────────────────────────')
    if (ctx.inputPath && fs.existsSync(ctx.inputPath)) {
        const analysisResult = await excelAgent.run(
            'excel-dashboard-session',
            `Analyse the workbook at "${ctx.inputPath}" using the analyse_workbook tool.`,
        )
        console.log(String(analysisResult))
    } else {
        console.log('  No input file provided – will use demo data.')
        ctx.sheetSummaries = [
            {
                name: 'SalesData',
                rowCount: 100,
                columns: ['Region', 'Product', 'Revenue', 'Units', 'Date'],
                sampleRows: [
                    { Region: 'North', Product: 'Widget A', Revenue: 15000, Units: 150, Date: '2025-01' },
                    { Region: 'South', Product: 'Widget B', Revenue: 22000, Units: 200, Date: '2025-01' },
                    { Region: 'East', Product: 'Widget A', Revenue: 18000, Units: 180, Date: '2025-02' },
                    { Region: 'West', Product: 'Widget C', Revenue: 12000, Units: 100, Date: '2025-02' },
                    { Region: 'North', Product: 'Widget B', Revenue: 25000, Units: 250, Date: '2025-03' },
                ],
                numericColumns: ['Revenue', 'Units'],
            },
        ]
    }
    console.log()

    console.log('─── Stage 2: Parse requirements ────────────────────────────')
    const reqResult = await requirementsAgent.run(
        'excel-dashboard-session',
        `Parse these dashboard requirements using the parse_requirements tool: "${args.requirements}"`,
    )
    console.log(String(reqResult))
    console.log()

    console.log('─── Stage 3: Create dashboard plan ─────────────────────────')
    const planResult = await plannerAgent.run(
        'excel-dashboard-session',
        `Create a dashboard plan using the create_dashboard_plan tool. Requirements: ${ctx.parsedRequirements}. Available columns: ${JSON.stringify(ctx.sheetSummaries.flatMap((s) => s.columns))}`,
    )
    console.log(String(planResult))
    console.log()

    console.log('─── Stage 4: Review plan ───────────────────────────────────')
    const reviewResult = await reviewAgent.run(
        'excel-dashboard-session',
        `Review this dashboard plan using the review_plan tool: ${ctx.dashboardPlan}`,
    )
    console.log(String(reviewResult))
    console.log()

    console.log('─── Stage 5: Build dashboard workbook ──────────────────────')
    const buildResult = await excelAgent.run(
        'excel-dashboard-session',
        `Build the dashboard using the build_dashboard tool. Plan: ${ctx.dashboardPlan}. Output to: ${ctx.outputPath}`,
    )
    console.log(String(buildResult))
    console.log()

    // --- Multi-agent runtime (optional, shows NativeMultiAgentRuntime) ----
    console.log('─── Agent Registry (NativeMultiAgentRuntime) ───────────────')
    const members = createOrchestratorMembers(model)
    const runtime = new NativeMultiAgentRuntime(members, workspaceHome)

    const allCards = (await runtime.registry()) as JsAgentCard[]
    console.log('  Registered agents:')
    for (const card of allCards) {
        console.log(
            `    • ${card.agentId} (${card.name}) – capabilities: [${card.capabilities.join(', ')}] status: ${card.status}`,
        )
    }

    const excelCards = (await runtime.discover('excel', JsAgentStatus.Online)) as JsAgentCard[]
    console.log(`\n  Excel-capable agents: ${excelCards.map((c) => c.name).join(', ')}`)
    console.log()

    console.log('═════════════════════════════════════════════════════════════')
    console.log(`  ✓ Dashboard written to: ${ctx.outputPath}`)
    console.log('═════════════════════════════════════════════════════════════')
}

main().catch((error: unknown) => {
    console.error('Pipeline failed:', error)
    process.exitCode = 1
})


