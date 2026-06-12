const readiness = [
  ["Trust Core", "Rust/Axum scaffold ready"],
  ["Orchestrator", "LangGraph mock ready"],
  ["Dashboard", "Next.js App Router ready"],
  ["Registry", "Foundry scaffold ready"],
];

export default function Home() {
  return (
    <main>
      <p className="eyebrow">AlethOS ToolPassport</p>
      <h1>Development workspace</h1>
      <p className="intro">
        The local-first mock path is initialized. Real model calls and onchain writes remain
        disabled until explicit human approval.
      </p>
      <section aria-label="Module readiness">
        {readiness.map(([name, status]) => (
          <article key={name}>
            <span>{name}</span>
            <strong>{status}</strong>
          </article>
        ))}
      </section>
    </main>
  );
}
