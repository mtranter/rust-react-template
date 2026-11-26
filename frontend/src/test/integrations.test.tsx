import { connection, connectionsApi } from "@/test/connections-fixtures";
import { holding } from "@/test/may-use-everything";
import { renderAt } from "@/test/render-app";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../../vitest.setup";

// Over the platform's `connections` screen, Integrations, which every app keeps after `make init`.

test("lists each Connection the modules declare with its status", async () => {
  server.use(
    ...connectionsApi([
      connection(),
      connection({ key: "erp", label: "ERP", url: "https://erp.example", status: "ok" }),
      connection({
        key: "mail",
        label: "Mail",
        url: "https://mail.example",
        status: "failing",
        lastError: "GET /health answered 503",
      }),
    ]).handlers,
  );
  renderAt("/integrations");

  const list = await screen.findByRole("region", { name: "Integrations" });
  const row = (name: string) => within(list).getByRole("row", { name: new RegExp(name) });
  await within(list).findByRole("link", { name: "CRM" });
  expect(within(row("CRM")).getByText("Not configured")).toBeInTheDocument();
  expect(within(row("ERP")).getByText("OK")).toBeInTheDocument();
  expect(within(row("ERP")).getByText("https://erp.example")).toBeInTheDocument();
  expect(within(row("Mail")).getByText("Failing")).toBeInTheDocument();
});

test("an admin enters a URL and a secret, tests the Connection and sees OK", async () => {
  const api = connectionsApi([connection()]);
  server.use(...api.handlers);
  const user = userEvent.setup();
  const { router } = renderAt("/integrations");

  await user.click(await screen.findByRole("link", { name: "CRM" }));
  expect(router.state.location.pathname).toBe("/integrations/crm");
  await user.type(await screen.findByLabelText("URL"), "https://crm.example/api");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(api.connections.get("crm")?.url).toBe("https://crm.example/api"));

  await user.type(screen.getByLabelText("Token"), "tok_live_working");
  await user.click(screen.getByRole("button", { name: "Set Token" }));
  expect(await screen.findByRole("button", { name: "Replace Token" })).toBeInTheDocument();
  expect(api.values.get("crm/token")).toBe("tok_live_working");

  await user.click(screen.getByRole("button", { name: "Test connection" }));
  const result = screen.getByRole("status", { name: "Test result" });
  expect(await within(result).findByText("OK")).toBeInTheDocument();
});

test("the secret field never shows a stored value", async () => {
  const api = connectionsApi([
    connection({
      url: "https://crm.example",
      status: "untested",
      secrets: [
        { name: "token", label: "Token", set: true, setAt: "2026-11-26T09:00:00Z", setBy: "ada" },
      ],
    }),
  ]);
  server.use(...api.handlers);
  const user = userEvent.setup();
  renderAt("/integrations/crm");

  const token = await screen.findByLabelText("Token");
  expect(token).toHaveValue("");
  expect(token).toHaveAttribute("type", "password");
  expect(screen.getByText(/Set by ada/)).toBeInTheDocument();

  await user.type(token, "tok_rotated");
  await user.click(screen.getByRole("button", { name: "Replace Token" }));
  await waitFor(() => expect(api.values.get("crm/token")).toBe("tok_rotated"));
  await waitFor(() => expect(screen.getByLabelText("Token")).toHaveValue(""));
  expect(screen.queryByDisplayValue("tok_rotated")).not.toBeInTheDocument();
});

test("a failing test says why in plain language", async () => {
  const api = connectionsApi([connection()]);
  server.use(...api.handlers);
  const user = userEvent.setup();
  renderAt("/integrations/crm");

  await user.type(await screen.findByLabelText("URL"), "https://crm.example");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(api.connections.get("crm")?.url).toBe("https://crm.example"));
  await user.type(screen.getByLabelText("Token"), "tok_wrong");
  await user.click(screen.getByRole("button", { name: "Set Token" }));
  await screen.findByRole("button", { name: "Replace Token" });
  await user.click(screen.getByRole("button", { name: "Test connection" }));

  const result = screen.getByRole("status", { name: "Test result" });
  expect(await within(result).findByText(/check the credentials/)).toBeInTheDocument();
});

test("Test connection waits until the Connection is filled in", async () => {
  server.use(...connectionsApi([connection()]).handlers);
  renderAt("/integrations/crm");

  expect(await screen.findByRole("button", { name: "Test connection" })).toBeDisabled();
  expect(screen.getByText(/Give it a URL and its credentials/)).toBeInTheDocument();
});

test("hides Integrations from a caller who may not list Connections, and shows the refusal", async () => {
  server.use(
    holding([{ resource: "subject", action: "list" }]),
    http.get("*/api/connections", () =>
      HttpResponse.json(
        { code: "forbidden", message: "forbidden", requestId: "test" },
        { status: 403 },
      ),
    ),
  );
  const { queryClient } = renderAt("/integrations");

  expect(await screen.findByText(/You may not manage integrations/)).toBeInTheDocument();
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  expect(screen.queryByRole("link", { name: /Integrations/ })).not.toBeInTheDocument();
});
