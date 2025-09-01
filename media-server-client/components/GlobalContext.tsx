import { createContext, ReactNode, useMemo, useState } from "react";

export const GlobalContext = createContext({});

export const GlobalContextProvider = ({
  children,
}: {
  children: ReactNode;
}) => {
  const [state, setState] = useState({});

  const value = useMemo(() => ({ state, setState }), [state, setState]);
  return (
    <GlobalContext.Provider value={value}>{children}</GlobalContext.Provider>
  );
};
