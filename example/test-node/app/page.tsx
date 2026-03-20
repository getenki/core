import { NativeEnkiAgent } from "@getenki/ai";
import { redirect } from "next/navigation";

const DEFAULT_MODEL = "ollama::llama3.2:latest";
const DEFAULT_PROMPT = "Explain what this project does.";
const DEFAULT_SYSTEM_PROMPT = "Answer clearly and keep responses short.";
const DEFAULT_MAX_ITERATIONS = 20;

type PageProps = {
  searchParams: Promise<{
    prompt?: string;
    response?: string;
    error?: string;
  }>;
};

function firstParam(value?: string | string[]) {
  if (Array.isArray(value)) {
    return value[0] ?? "";
  }

  return value ?? "";
}

export default async function Home({ searchParams }: PageProps) {
  const params = await searchParams;
  const prompt = firstParam(params.prompt) || DEFAULT_PROMPT;
  const response = firstParam(params.response);
  const error = firstParam(params.error);

  async function runEnki(formData: FormData) {
    "use server";

    const prompt = String(formData.get("prompt") ?? "").trim();

    if (!prompt) {
      redirect("/?error=Prompt%20is%20required.");
    }

    try {
      const model = (process.env.ENKI_MODEL ?? DEFAULT_MODEL).trim();

      const agent = new NativeEnkiAgent(
        "Agent",
        DEFAULT_SYSTEM_PROMPT,
        model,
        DEFAULT_MAX_ITERATIONS,
        process.cwd(),
      );

      const output = await agent.run("next-server-action-demo", prompt);
      const nextParams = new URLSearchParams({
        prompt,
        response: output,
      });

      redirect(`/?${nextParams.toString()}`);
    } catch (error) {
      const nextParams = new URLSearchParams({
        prompt,
        error: error instanceof Error ? error.message : "Unknown Enki error.",
      });

      redirect(`/?${nextParams.toString()}`);
    }
  }

  return (
    <main className="min-h-screen bg-zinc-950 px-6 py-16 text-zinc-100">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-8">
        <div className="space-y-3">
          <p className="text-sm uppercase tracking-[0.3em] text-zinc-500">
            Next.js Server Action
          </p>
          <h1 className="text-4xl font-semibold tracking-tight">
            Run Enki from the server
          </h1>
          <p className="max-w-2xl text-sm leading-6 text-zinc-400">
            This form submits to an inline server action, runs the local
            `enki-js` binding on the server, and renders the result after a
            redirect.
          </p>
        </div>

        <form action={runEnki} className="space-y-4 rounded-3xl border border-zinc-800 bg-zinc-900 p-6">
          <label className="block space-y-2">
            <span className="text-sm font-medium text-zinc-300">Prompt</span>
            <textarea
              name="prompt"
              defaultValue={prompt}
              rows={5}
              className="w-full rounded-2xl border border-zinc-700 bg-zinc-950 px-4 py-3 text-sm text-zinc-100 outline-none transition focus:border-zinc-500"
            />
          </label>

          <button
            type="submit"
            className="rounded-full bg-zinc-100 px-5 py-2 text-sm font-medium text-zinc-950 transition hover:bg-zinc-300"
          >
            Run server action
          </button>
        </form>

        {error ? (
          <section className="rounded-3xl border border-red-900/80 bg-red-950/40 p-6">
            <h2 className="text-sm font-semibold uppercase tracking-[0.2em] text-red-300">
              Error
            </h2>
            <p className="mt-3 whitespace-pre-wrap text-sm leading-6 text-red-100">
              {error}
            </p>
          </section>
        ) : null}

        {response ? (
          <section className="rounded-3xl border border-zinc-800 bg-zinc-900 p-6">
            <h2 className="text-sm font-semibold uppercase tracking-[0.2em] text-zinc-400">
              Response
            </h2>
            <p className="mt-3 whitespace-pre-wrap text-sm leading-6 text-zinc-100">
              {response}
            </p>
          </section>
        ) : null}
      </div>
    </main>
  );
}
