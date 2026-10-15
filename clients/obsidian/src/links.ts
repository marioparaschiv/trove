/** The note's text with `link` on a line of its own at the end; unchanged when a line already holds just the link. */
export function appendLink(text: string, link: string): string {
  if (text.split("\n").some((line) => line.trim() === link)) {
    return text;
  }

  if (text === "") {
    return `${link}\n`;
  }

  return `${text}${text.endsWith("\n") ? "" : "\n"}\n${link}\n`;
}
