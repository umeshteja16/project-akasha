import { useQuery } from "@tanstack/react-query";
import { MessageSquareQuoteIcon } from "lucide-react";
import { useApi } from "@/api/context";
import { metaQuery } from "@/api/queries";
import { EmptyState } from "@/components/common/empty-state";
import { PageHeader } from "@/components/common/page-header";

export function ChatPage() {
  const api = useApi();
  const meta = useQuery(metaQuery(api));
  return (
    <div className="grid gap-8">
      <PageHeader
        eyebrow="Ask"
        title="Chat"
        description="Questions answered from your own files, with every claim linked to the passage it came from."
      />
      <EmptyState icon={MessageSquareQuoteIcon} title="Ask your library">
        <p>
          When the evidence is thin, Akasha says so instead of guessing.
          {meta.data && !meta.data.chat_model
            ? " This server has no language model configured, so answers will be the best passages themselves."
            : null}
        </p>
      </EmptyState>
    </div>
  );
}
