const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * The part of `text` from byte `start` to byte `end` of its UTF-8 encoding. Search results give every offset in UTF-8
 * bytes (a match's `byte_start` and `byte_end` in its file, a highlight's `start` and `end` in its snippet), which
 * differ from JavaScript's UTF-16 string indices once the text has a character outside ASCII.
 */
export function sliceBytes(text: string, start: number, end?: number): string {
  return decoder.decode(encoder.encode(text).subarray(start, end));
}
