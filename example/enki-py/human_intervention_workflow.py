import asyncio
import json
from pathlib import Path

import enki_py


MODEL = 'ollama::qwen3.5:latest'
WORKSPACE_HOME = Path('./example/enki-py/.enki-human-intervention')


def build_placeholder_agents() -> list[enki_py.EnkiAgent]:
    observer = enki_py.EnkiAgent(
        name='Workflow Observer',
        system_prompt_preamble='You are a placeholder workflow agent and should never be called in this example.',
        model=MODEL,
        max_iterations=1,
        workspace_home=str(WORKSPACE_HOME),
    )
    observer.configure_workflow(
        agent_id='workflow-observer',
        capabilities=['support'],
    )
    return [observer]


def build_human_gate_workflow() -> list[str]:
    return [
        json.dumps(
            {
                'id': 'approval-flow',
                'name': 'Approval Flow',
                'nodes': [
                    {
                        'id': 'approval',
                        'kind': 'human_gate',
                        'prompt': 'Approve publishing these release notes?',
                        'output_key': 'approval',
                    }
                ],
                'edges': [],
            }
        )
    ]


def build_failure_intervention_tasks() -> list[str]:
    return [
        json.dumps(
            {
                'id': 'missing-agent-task',
                'target': {'type': 'agent_id', 'value': 'missing-agent'},
                'prompt': 'This task intentionally targets a missing agent.',
                'failure_policy': 'pause_for_intervention',
            }
        )
    ]


def build_failure_intervention_workflow() -> list[str]:
    return [
        json.dumps(
            {
                'id': 'failure-escalation-flow',
                'name': 'Failure Escalation Flow',
                'nodes': [
                    {
                        'id': 'missing-agent-step',
                        'kind': 'task',
                        'task_id': 'missing-agent-task',
                        'output_key': 'missing_agent_step',
                    }
                ],
                'edges': [],
            }
        )
    ]


def prompt_for_intervention(prompt: str, allowed: str) -> str:
    print(f'\nHuman input required: {prompt}')
    print(f'Allowed responses: {allowed}')
    return input('Your response: ').strip()


async def run_human_gate_example() -> None:
    runtime = enki_py.EnkiWorkflowRuntime(
        agents=build_placeholder_agents(),
        tasks_json=[],
        workflows_json=build_human_gate_workflow(),
        workspace_home=str(WORKSPACE_HOME),
    )

    response = json.loads(
        await runtime.start_json(
            json.dumps({'workflow_id': 'approval-flow', 'input': {'requester': 'release-bot'}})
        )
    )

    print('Human gate response:')
    print(json.dumps(response, indent=2))

    paused = json.loads(await runtime.inspect_json(response['run_id']))
    print('\nPending interventions for human gate:')
    print(json.dumps(paused['pending_interventions'], indent=2))

    intervention = paused['pending_interventions'][0]
    intervention_id = intervention['id']
    human_response = prompt_for_intervention(intervention['prompt'], 'yes / no')
    resolved = json.loads(
        await runtime.submit_intervention_json(response['run_id'], intervention_id, human_response)
    )
    print('\nState after submitting human response:')
    print(json.dumps(resolved, indent=2))

    resumed = json.loads(await runtime.resume_json(response['run_id']))
    print('\nResumed human gate workflow:')
    print(json.dumps(resumed, indent=2))

    persisted = json.loads(await runtime.inspect_json(response['run_id']))
    print('\nPersisted human gate state:')
    print(json.dumps(persisted, indent=2))


async def run_failure_escalation_example() -> None:
    runtime = enki_py.EnkiWorkflowRuntime(
        agents=build_placeholder_agents(),
        tasks_json=build_failure_intervention_tasks(),
        workflows_json=build_failure_intervention_workflow(),
        workspace_home=str(WORKSPACE_HOME),
    )

    response = json.loads(
        await runtime.start_json(
            json.dumps({'workflow_id': 'failure-escalation-flow', 'input': {'ticket': 'OPS-42'}})
        )
    )

    print('\nFailure escalation response:')
    print(json.dumps(response, indent=2))

    paused = json.loads(await runtime.inspect_json(response['run_id']))
    print('\nPending interventions for failed node:')
    print(json.dumps(paused['pending_interventions'], indent=2))

    intervention = paused['pending_interventions'][0]
    intervention_id = intervention['id']
    human_response = prompt_for_intervention(
        intervention['prompt'],
        'retry / skip / continue / fail',
    )
    resolved = json.loads(
        await runtime.submit_intervention_json(response['run_id'], intervention_id, human_response)
    )
    print('\nState after submitting human response:')
    print(json.dumps(resolved, indent=2))

    resumed = json.loads(await runtime.resume_json(response['run_id']))
    print('\nResumed failure escalation workflow:')
    print(json.dumps(resumed, indent=2))

    persisted = json.loads(await runtime.inspect_json(response['run_id']))
    print('\nPersisted failure escalation state:')
    print(json.dumps(persisted, indent=2))


async def main() -> None:
    WORKSPACE_HOME.mkdir(parents=True, exist_ok=True)

    await run_human_gate_example()
    await run_failure_escalation_example()


if __name__ == '__main__':
    asyncio.run(main())
