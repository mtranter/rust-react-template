import { notesApi } from "@/test/fixtures";
import { renderAt } from "@/test/render-app";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, onTestFinished, test } from "vitest";
import { server } from "../../vitest.setup";

test("choosing the dark theme darkens the page and is remembered", async () => {
  onTestFinished(() => {
    localStorage.clear();
    document.documentElement.classList.remove("dark");
  });
  server.use(...notesApi([]).handlers);
  const user = userEvent.setup();
  renderAt("/");

  await user.click(await screen.findByRole("button", { name: "Theme" }));
  await user.click(await screen.findByRole("menuitemradio", { name: "Dark" }));

  await waitFor(() => expect(document.documentElement).toHaveClass("dark"));
  expect(localStorage.getItem("app-theme")).toBe("dark");
});
