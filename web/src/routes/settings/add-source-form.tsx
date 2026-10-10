import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FolderPlusIcon } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import { unwrap } from "@/api/client";
import { useApi } from "@/api/context";
import { fileKeys } from "@/api/files";
import { parseGlobs, type SourceOnDelete, sourceKeys } from "@/api/sources";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { Textarea } from "@/components/ui/textarea";
import { toast } from "@/components/ui/toast";
import { sentence } from "@/lib/session";
import { FormError } from "@/routes/auth/form-error";

const ON_DELETE = [
  { value: "delete", label: "Remove it here too" },
  { value: "keep", label: "Keep it" },
] as const;

/** Add a watched folder under one of the server's allowed roots. */
export function AddSourceForm({ roots }: { roots: string[] }) {
  const api = useApi();
  const queryClient = useQueryClient();
  const ids = useId();
  const [path, setPath] = useState(roots.length === 1 ? `${roots[0]}/` : "");
  const [name, setName] = useState("");
  const [onDelete, setOnDelete] = useState<SourceOnDelete>("delete");
  const [include, setInclude] = useState("");
  const [exclude, setExclude] = useState("");
  const add = useMutation({
    mutationFn: () =>
      unwrap(
        api.POST("/api/v1/sources", {
          body: {
            path: path.trim().replace(/(.)\/+$/, "$1"),
            name: name.trim() || null,
            on_delete: onDelete,
            include_globs: parseGlobs(include),
            exclude_globs: parseGlobs(exclude),
          },
        }),
      ),
    onSuccess: (source) => {
      setName("");
      setInclude("");
      setExclude("");
      void queryClient.invalidateQueries({ queryKey: sourceKeys.all });
      void queryClient.invalidateQueries({ queryKey: fileKeys.all });
      toast({ title: `Watching “${source.name}”: importing its files`, tone: "success" });
    },
  });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (path.trim()) add.mutate();
  };
  return (
    <Card>
      <form onSubmit={submit}>
        <CardContent className="grid gap-5">
          <FormError message={add.isError ? sentence(add.error.message) : null} />
          <Field
            label="Folder on the server"
            name="source_path"
            autoComplete="off"
            spellCheck={false}
            required
            className="font-mono"
            placeholder={roots[0] ? `${roots[0]}/Notes` : "/data/notes"}
            value={path}
            onChange={(e) => setPath(e.target.value)}
            hint={
              <>
                Inside {roots.length === 1 ? "" : "one of "}
                {roots.map((root, i) => (
                  <span key={root}>
                    {i > 0 ? ", " : null}
                    <code className="font-mono">{root}</code>
                  </span>
                ))}
              </>
            }
          />
          <Field
            label="Name"
            name="source_name"
            autoComplete="off"
            maxLength={100}
            placeholder="The folder's name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <div className="grid gap-1.5">
            <span className="text-sm font-medium text-fg">
              When a file is deleted from the folder
            </span>
            <Segmented
              label="When a file is deleted from the folder"
              value={onDelete}
              onValueChange={setOnDelete}
              options={ON_DELETE}
            />
          </div>
          <details className="group grid gap-3">
            <summary className="cursor-pointer text-sm font-medium text-fg-muted hover:text-fg">
              Only some files
            </summary>
            <div className="grid gap-4 pt-3">
              <div className="grid gap-1.5">
                <Label htmlFor={`${ids}-include`}>Include (one pattern per line)</Label>
                <Textarea
                  id={`${ids}-include`}
                  rows={2}
                  spellCheck={false}
                  className="font-mono"
                  placeholder="**/*.md"
                  value={include}
                  onChange={(e) => setInclude(e.target.value)}
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor={`${ids}-exclude`}>Leave out</Label>
                <Textarea
                  id={`${ids}-exclude`}
                  rows={2}
                  spellCheck={false}
                  className="font-mono"
                  placeholder="Archive/**"
                  value={exclude}
                  onChange={(e) => setExclude(e.target.value)}
                />
              </div>
              <p className="text-xs text-fg-subtle">
                Hidden files and folders such as <code className="font-mono">.obsidian</code> and{" "}
                <code className="font-mono">.trash</code> are always left out.
              </p>
            </div>
          </details>
        </CardContent>
        <CardFooter>
          <Button type="submit" disabled={!path.trim() || add.isPending}>
            <FolderPlusIcon aria-hidden />
            {add.isPending ? "Adding…" : "Watch folder"}
          </Button>
        </CardFooter>
      </form>
    </Card>
  );
}
