export default function Placeholder({ title, emoji, desc }: { title: string; emoji: string; desc: string }) {
  return (
    <div className="screen">
      <div className="placeholder-screen">
        <div className="emoji">{emoji}</div>
        <h2>{title}</h2>
        <p>{desc}</p>
      </div>
    </div>
  );
}
