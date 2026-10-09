import { BookmarkIcon, PlusIcon, Trash2Icon } from "lucide-react";
import type { ReactNode } from "react";
import { PageHeader } from "@/components/common/page-header";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Kbd } from "@/components/ui/kbd";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toast } from "@/components/ui/toast";
import { Tooltip } from "@/components/ui/tooltip";
import { useDocumentTitle } from "@/lib/use-document-title";

const SWATCHES = [
  "bg",
  "surface",
  "surface-2",
  "sidebar",
  "border",
  "border-strong",
  "fg",
  "fg-muted",
  "fg-subtle",
  "accent",
  "accent-soft",
  "highlight",
  "mark",
  "danger",
  "danger-soft",
] as const;

function Block({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid gap-4 border-b border-border py-8">
      <h2 className="eyebrow">{title}</h2>
      {children}
    </section>
  );
}

/** Living reference of the design system (web/DESIGN.md). Not linked in the nav. */
export function DesignPage() {
  useDocumentTitle("Design system");
  return (
    <div>
      <PageHeader
        eyebrow="Reference"
        title="Design system"
        description="Tokens and primitives as they render in the current theme."
      />
      <Block title="Colour">
        <div className="grid grid-cols-3 gap-3 sm:grid-cols-5">
          {SWATCHES.map((name) => (
            <div key={name} className="grid gap-1.5">
              <div
                className="h-12 rounded-md border border-border"
                style={{ background: `var(--${name})` }}
              />
              <code className="font-mono text-2xs text-fg-muted">--{name}</code>
            </div>
          ))}
        </div>
      </Block>
      <Block title="Type">
        <p className="display text-4xl">A quiet place to remember</p>
        <p className="display text-2xl italic">Display, Newsreader, italic for the wordmark</p>
        <p className="text-base">
          Reading text, Inter 16/26. Answers and extracted text use this size.
        </p>
        <p className="text-sm text-fg-muted">UI text, Inter 14/22. Labels, menus, tables.</p>
        <p className="font-mono text-sm">Excerpts &amp; code: JetBrains Mono — p. 14, ¶ 3</p>
        <p className="eyebrow">Eyebrow label · mono caps</p>
        <p className="text-sm">
          Search hits look like <mark>this highlighted</mark> passage.
        </p>
      </Block>
      <Block title="Buttons">
        <div className="flex flex-wrap items-center gap-3">
          <Button>
            <PlusIcon /> Primary
          </Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="danger">
            <Trash2Icon /> Danger
          </Button>
          <Button variant="link">Link</Button>
          <Button size="sm" variant="secondary">
            Small
          </Button>
          <Tooltip content="Pin to top">
            <Button size="icon" variant="secondary" aria-label="Pin">
              <BookmarkIcon />
            </Button>
          </Tooltip>
          <Button disabled>Disabled</Button>
        </div>
      </Block>
      <Block title="Inputs, badges, keys">
        <div className="grid max-w-xl gap-5 sm:grid-cols-2">
          <Field label="Label" placeholder="Placeholder" hint="Helpful hint text." />
          <Field label="With error" defaultValue="too short" error="Use at least 8 characters." />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge>pdf</Badge>
          <Badge tone="accent">ready</Badge>
          <Badge tone="danger">failed</Badge>
          <span className="ml-2 flex gap-1">
            <Kbd>⌘</Kbd>
            <Kbd>K</Kbd>
          </span>
        </div>
      </Block>
      <Block title="Surfaces">
        <div className="grid gap-4 sm:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle>Card title</CardTitle>
              <CardDescription>
                Cards sit on the paper with a hairline and a faint shadow.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <Tabs defaultValue="one">
                <TabsList>
                  <TabsTrigger value="one">Overview</TabsTrigger>
                  <TabsTrigger value="two">Text</TabsTrigger>
                </TabsList>
                <TabsContent value="one" className="text-sm text-fg-muted">
                  Tab one content.
                </TabsContent>
                <TabsContent value="two" className="text-sm text-fg-muted">
                  Tab two content.
                </TabsContent>
              </Tabs>
            </CardContent>
          </Card>
          <Card>
            <CardContent className="grid gap-3">
              <Skeleton className="h-5 w-1/2" />
              <Skeleton className="h-4" />
              <Skeleton className="h-4 w-5/6" />
              <div className="flex gap-2 pt-2">
                <Dialog>
                  <DialogTrigger asChild>
                    <Button variant="secondary" size="sm">
                      Open dialog
                    </Button>
                  </DialogTrigger>
                  <DialogContent>
                    <DialogHeader>
                      <DialogTitle>Dialog title</DialogTitle>
                      <DialogDescription>Dialogs rise in and trap focus.</DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                      <DialogClose asChild>
                        <Button variant="secondary">Close</Button>
                      </DialogClose>
                    </DialogFooter>
                  </DialogContent>
                </Dialog>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() =>
                    toast({ title: "Saved", description: "A success toast.", tone: "success" })
                  }
                >
                  Show toast
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      </Block>
    </div>
  );
}
