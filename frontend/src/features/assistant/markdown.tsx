import ReactMarkdown from "react-markdown";

/**
 * The assistant's words as markdown. Safe for text a tool result may have steered: HTML in it is
 * dropped rather than rendered (`skipHtml`), and react-markdown's default URL transform drops
 * `javascript:` and other unsafe links.
 */
export function Markdown({ text }: { text: string }) {
  return (
    <div className="space-y-2 break-words [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:font-mono [&_code]:text-sm [&_h1]:font-semibold [&_h2]:font-semibold [&_h3]:font-semibold [&_ol]:list-decimal [&_ol]:pl-5 [&_pre]:overflow-x-auto [&_pre]:rounded [&_pre]:bg-muted [&_pre]:p-2 [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_ul]:list-disc [&_ul]:pl-5">
      <ReactMarkdown skipHtml>{text}</ReactMarkdown>
    </div>
  );
}
