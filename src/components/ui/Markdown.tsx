import { useMemo } from 'react';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { browse } from '@/lib/shell';
import { cn } from '@/lib/cn';

marked.setOptions({ gfm: true, breaks: true });

/** Sanitised Markdown (AI answers). Links open in the browser. */
export function Markdown({ text, className }: { text: string; className?: string }) {
  const html = useMemo(() => DOMPurify.sanitize(marked.parse(text, { async: false }) as string), [text]);
  return (
    <div
      className={cn('md', className)}
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={(e) => {
        const a = (e.target as HTMLElement).closest('a');
        if (!a) return;
        e.preventDefault();
        const href = a.getAttribute('href') ?? '';
        if (/^https?:\/\//.test(href)) browse(href);
      }}
    />
  );
}
