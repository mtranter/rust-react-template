import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ChevronDownIcon } from "lucide-react";

export type MultiSelectOption = { value: string; label: string };
export type MultiSelectGroup = { label?: string; options: MultiSelectOption[] };

type Props = {
  id: string;
  groups: MultiSelectGroup[];
  value: string[];
  onChange: (value: string[]) => void;
  placeholder: string;
  disabled?: boolean;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
};

/**
 * Chooses any number of options, as checkboxes in a popover, with a "Select all" that is
 * indeterminate while some are chosen. The trigger, labelled by the caller's `<label htmlFor={id}>`,
 * sums up the choice ("3 selected").
 */
export function MultiSelect({
  id,
  groups,
  value,
  onChange,
  placeholder,
  disabled,
  ...aria
}: Props) {
  const options = groups.flatMap((g) => g.options);
  const chosen = options.filter((o) => value.includes(o.value));
  const all = options.length > 0 && chosen.length === options.length;
  const summary =
    chosen.length === 0
      ? placeholder
      : chosen.length === 1
        ? chosen[0].label
        : `${chosen.length} selected`;
  const toggle = (option: string, on: boolean) =>
    onChange(on ? [...value, option] : value.filter((v) => v !== option));

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            id={id}
            variant="outline"
            className="w-full justify-between font-normal"
            disabled={disabled}
            {...aria}
          />
        }
      >
        <span className={chosen.length === 0 ? "text-muted-foreground" : undefined}>{summary}</span>
        <ChevronDownIcon />
      </PopoverTrigger>
      <PopoverContent align="start" className="max-h-80 overflow-y-auto">
        <fieldset className="grid gap-2">
          <legend className="sr-only">{placeholder}</legend>
          {/* biome-ignore lint/a11y/noLabelWithoutControl: the checkbox inside is the control */}
          <label className="flex items-center gap-2 font-medium">
            <Checkbox
              checked={all}
              indeterminate={chosen.length > 0 && !all}
              onCheckedChange={(on) => onChange(on ? options.map((o) => o.value) : [])}
            />
            Select all
          </label>
          {groups.map((group, index) => (
            <div key={group.label ?? index} className="grid gap-2">
              {group.label && (
                <div className="pt-1 text-xs text-muted-foreground">{group.label}</div>
              )}
              {group.options.map((option) => (
                // biome-ignore lint/a11y/noLabelWithoutControl: the checkbox inside is the control
                <label key={option.value} className="flex items-center gap-2">
                  <Checkbox
                    checked={value.includes(option.value)}
                    onCheckedChange={(on) => toggle(option.value, on)}
                  />
                  {option.label}
                </label>
              ))}
            </div>
          ))}
        </fieldset>
      </PopoverContent>
    </Popover>
  );
}
