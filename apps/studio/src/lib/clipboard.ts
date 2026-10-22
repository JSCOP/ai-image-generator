// Copying text (prompts) to the system clipboard.

/** The prompt as the service sends it: "prompt\nAvoid: avoid" (compose() in service/generate.rs). */
export function promptText(prompt: string, avoid = ''): string {
  const p = prompt.trim();
  const a = avoid.trim();
  return a ? `${p}\nAvoid: ${a}` : p;
}

/** False when the clipboard is unavailable or refuses; never throws. */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (!navigator.clipboard?.writeText) return false;
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
