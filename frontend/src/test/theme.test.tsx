import { renderAt } from "@/test/render-app";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, onTestFinished, test } from "vitest";

test("choosing the dark theme darkens the page and is remembered", async () => {
  onTestFinished(() => {
    localStorage.clear();
    document.documentElement.classList.remove("dark");
  });
  const user = userEvent.setup();
  renderAt("/");

  await user.click(await screen.findByRole("button", { name: "Theme" }));
  await user.click(await screen.findByRole("menuitemradio", { name: "Dark" }));

  await waitFor(() => expect(document.documentElement).toHaveClass("dark"));
  expect(localStorage.getItem("app-theme")).toBe("dark");
});
