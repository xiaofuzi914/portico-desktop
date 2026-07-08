import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { listSkills } from "@/lib/tauri-api";
import { featureReadiness } from "@/lib/feature-readiness";
import type { Skill } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { skillKeys } from "@/lib/query-keys";

export function SkillCapabilitiesPanel() {
  const { t } = useTranslation();
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [argumentsJson, setArgumentsJson] = useState("{}");

  const { data: skills, isLoading } = useQuery({
    queryKey: skillKeys.list(),
    queryFn: () => listSkills(),
  });

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.availableSkills")}</CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <p className="text-muted-foreground">{t("capabilities.loadingSkills")}</p>
          ) : skills?.length ? (
            <ul className="divide-y">
              {skills.map((skill) => (
                <li
                  key={skill.id}
                  className={`flex cursor-pointer items-start justify-between gap-4 py-4 ${
                    selectedSkill?.id === skill.id ? "bg-muted/50" : ""
                  }`}
                  onClick={() => {
                    setSelectedSkill(skill);
                    setArgumentsJson("{}");
                  }}
                >
                  <div className="flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{skill.name}</span>
                      <span className="text-muted-foreground text-xs">{skill.plugin_id}</span>
                    </div>
                    <p className="text-muted-foreground text-sm">{skill.description}</p>
                    <p className="text-muted-foreground text-xs">
                      {t("capabilities.trigger")} {skill.trigger_description}
                    </p>
                    {skill.required_tools.length > 0 && (
                      <p className="text-muted-foreground text-xs">
                        {t("capabilities.requiredTools")} {skill.required_tools.join(", ")}
                      </p>
                    )}
                  </div>
                  <Button
                    size="sm"
                    variant={selectedSkill?.id === skill.id ? "default" : "outline"}
                    onClick={(e) => {
                      e.stopPropagation();
                      setSelectedSkill(skill);
                      setArgumentsJson("{}");
                    }}
                  >
                    {t("capabilities.inspect")}
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">{t("capabilities.noSkills")}</p>
          )}
        </CardContent>
      </Card>

      {selectedSkill && (
        <Card>
          <CardHeader>
            <CardTitle>
              {t("capabilities.invoke")} {selectedSkill.name}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-muted-foreground text-sm">{selectedSkill.description}</p>
            <div>
              <label className="mb-1 block text-sm font-medium">
                {t("capabilities.argumentsJson")}
              </label>
              <Textarea
                value={argumentsJson}
                onChange={(e) => setArgumentsJson(e.target.value)}
                rows={4}
              />
            </div>
            <Button disabled>
              {featureReadiness.skillInvocation.ready
                ? t("capabilities.runSkill")
                : t("capabilities.invocationUnavailable")}
            </Button>
            <p className="text-muted-foreground text-sm">
              {t("capabilities.skillInvocationReason")}
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
