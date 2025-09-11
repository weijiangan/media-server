import {
  Dimensions,
  FlatList,
  ImageBackground,
  StyleSheet,
  TouchableOpacity,
} from "react-native";

import { ThemedText } from "@/components/ThemedText";
import { ThemedView } from "@/components/ThemedView";
import { useContext, useEffect, useRef, useState } from "react";
import {
  Href,
  Link,
  Stack,
  useFocusEffect,
  useLocalSearchParams,
} from "expo-router";
import { API_BASE_URL } from "@/constants/Config";
import { GlobalContext } from "@/components/GlobalContext";

// eslint-disable-next-line @typescript-eslint/no-unused-vars
const statuses = ["init", "loading", "success", "error"] as const;
type Status = (typeof statuses)[number];

export default function HomeScreen() {
  const { globalState, setGlobalState } = useContext(GlobalContext);
  const searchParams = useLocalSearchParams();
  const currentPathId = (searchParams.pathId as string) ?? "/";
  const prevPathId = useRef(currentPathId);
  const status = useRef<Status>("init");

  useEffect(() => {
    if (prevPathId.current === currentPathId && status.current !== "init") {
      return;
    }

    let url = new URL(`${API_BASE_URL}/media`);
    if (currentPathId !== "/") {
      url.searchParams.append("parent_id", currentPathId);
    }

    let options = {
      method: "GET",
      headers: {
        Accept: "*/*",
        "Accept-Encoding": "gzip, deflate, br",
        Connection: "keep-alive",
      },
    };

    status.current = "loading";
    fetch(url, options)
      .then((res) => res.json())
      .then((json) => {
        setGlobalState((prev) => ({
          ...prev,
          files: { ...prev.files, [currentPathId]: json.files },
        }));
        status.current = "success";
      })
      .catch((err) => {
        console.error("error:" + err);
        status.current = "error";
      });

    prevPathId.current = currentPathId;
  });

  const { width } = Dimensions.get("window");
  const [numColumns, setNumColumns] = useState(Math.floor(width / 115));

  useFocusEffect(() => {
    const newNumColumns = Math.floor(width / 115);
    if (newNumColumns !== numColumns) {
      setNumColumns(newNumColumns);
    }
  });

  return (
    <ThemedView style={{ flex: 1 }}>
      <Stack.Screen options={{ title: currentPathId ?? "Files" }} />
      <FlatList
        numColumns={numColumns}
        contentContainerStyle={{ gap: 1 }}
        columnWrapperStyle={{ gap: 1 }}
        data={globalState?.files?.[currentPathId]?.filter(
          (item) =>
            item.mime_type?.startsWith("image/") ||
            item.mime_type?.startsWith("video/") ||
            item.mime_type === null
        )}
        renderItem={({ item }) => {
          let href: Href = "/files";
          if (
            item.mime_type?.startsWith("video/") ||
            item.mime_type?.startsWith("image/")
          ) {
            href = {
              pathname: `/media-viewer/[fileId]`,
              params: { fileId: item.id, pathId: currentPathId },
            };
          } else if (item.type === "directory") {
            href = { pathname: `/files`, params: { pathId: item.id } };
          }

          return (
            <Link push href={href} asChild>
              <TouchableOpacity
                style={{
                  aspectRatio: 3 / 4,
                  width: (width - (numColumns - 1)) / numColumns,
                }}
              >
                {item.mime_type ? (
                  <ImageBackground
                    resizeMode="cover"
                    style={{ flex: 1 }}
                    source={{
                      uri:
                        (item.thumb_path &&
                          `${API_BASE_URL}${item.thumb_path}`) ||
                        `${API_BASE_URL}/media/thumbnail?id=${item.id}`,
                    }}
                  />
                ) : (
                  <ThemedView
                    style={{
                      flex: 1,
                      alignItems: "center",
                      justifyContent: "center",
                    }}
                  >
                    <ThemedText>{item.name}</ThemedText>
                  </ThemedView>
                )}
              </TouchableOpacity>
            </Link>
          );
        }}
      />
    </ThemedView>
  );
}

const styles = StyleSheet.create({});
