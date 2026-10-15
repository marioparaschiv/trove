/** The note's text with `link` on a line of its own at the end. */
export function appendLink(text: string, link: string): string {
  if (text === "") {
    return `${link}\n`;
  }

  return `${text}${text.endsWith("\n") ? "" : "\n"}\n${link}\n`;
}
