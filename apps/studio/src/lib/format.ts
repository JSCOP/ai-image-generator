/** "1920x1080" -> "1920×1080" (any other text is returned as is). */
export function formatSize(size: string): string {
  return size.replace(/^(\d+)\s*[xX×]\s*(\d+)$/, '$1×$2');
}
