/**
 * Shared remark/rehype plugins for GFM + KaTeX.
 * Stable arrays so ReactMarkdown does not remount the tree every stream tick.
 */
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";

export const MARKDOWN_REMARK_PLUGINS = [remarkGfm, remarkMath];
export const MARKDOWN_REHYPE_PLUGINS = [rehypeKatex];
