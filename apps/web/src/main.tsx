import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";

import { Providers } from "./app/providers";
import { router } from "./app/router";
import "./styles/index.css";

const container = document.getElementById("application");

if (container === null) {
  throw new Error("the application container element is missing from the document");
}

createRoot(container).render(
  <StrictMode>
    <Providers>
      <RouterProvider router={router} />
    </Providers>
  </StrictMode>,
);
