import { Languages } from "lucide-react";
import { Button } from "@/components/ui/button";
import { getLanguageLabel } from "@/lib/i18n";
import { useTranslation } from "@/lib/i18n-react";

export function LanguageToggle() {
  const { language, toggleLanguage, t } = useTranslation();

  return (
    <Button
      variant="outline"
      size="sm"
      onClick={toggleLanguage}
      aria-label={t("language.label")}
      title={t("language.label")}
      className="h-8 px-2"
    >
      <Languages className="h-3.5 w-3.5" />
      <span className="hidden sm:inline">{getLanguageLabel(language)}</span>
      <span className="sm:hidden">{language.toUpperCase()}</span>
    </Button>
  );
}
