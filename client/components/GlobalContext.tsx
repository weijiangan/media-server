import { createContext, ReactNode, useMemo, useState } from "react";

interface MediaItem {
  created_at: string;
  duration_secs: number;
  height: number;
  id: number;
  mime_type: string | null;
  name: string;
  parent_id: number | null;
  path: string;
  size: number;
  tags: string | null;
  thumb_path: string | null;
  type: "file" | "directory";
  width: number;
  kind: "image" | "video" | "audio" | "other";
}

export interface GlobalState {
  files: { [pathId: string]: MediaItem[] };
}

export const GlobalContext = createContext<{
  globalState: GlobalState;
  setGlobalState: React.Dispatch<React.SetStateAction<GlobalState>>;
}>({ globalState: { files: {} }, setGlobalState: () => {} });

export const GlobalContextProvider = ({
  children,
}: {
  children: ReactNode;
}) => {
  const [globalState, setGlobalState] = useState({ files: {} });

  const value = useMemo(
    () => ({ globalState, setGlobalState }),
    [globalState, setGlobalState]
  );
  return (
    <GlobalContext.Provider value={value}>{children}</GlobalContext.Provider>
  );
};
