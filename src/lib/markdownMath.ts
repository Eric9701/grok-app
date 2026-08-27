/**
 * Shared remark/rehype plugins for chat + preview markdown.
 * GFM + KaTeX ($…$ / $$…$$ and \(…\) / \[…\]).
 */

import type { Options } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";

export const MARKDOWN_REMARK_PLUGINS: NonNullable<Options["remarkPlugins"]> = [
  remarkGfm,
  remarkMath,
];

export const MARKDOWN_REHYPE_PLUGINS: NonNullable<Options["rehypePlugins"]> = [
  [
    rehypeKatex,
    {
      throwOnError: false,
      errorColor: "#c44",
      strict: "ignore",
      output: "html",
      trust: false,
    },
  ],
];
