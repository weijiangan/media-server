import { Dimensions, Image, Pressable } from "react-native";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";
import { API_BASE_URL } from "@/constants/Config";
import { useContext, useEffect, useRef, useState } from "react";
import { GlobalContext, GlobalState } from "@/components/GlobalContext";
import styles from "@/styles/viewer.module.css";

const ImageViewer = ({ fileId }: { fileId: string }) => {
  return (
    <Image
      source={{ uri: `${API_BASE_URL}/media/image?id=${fileId}` }}
      style={{ flex: 1 }}
      resizeMode="contain"
    />
  );
};

const VideoPlayer = ({ fileId }: { fileId: string }) => {
  return (
    <video controls poster={`${API_BASE_URL}/media/thumbnail?id=${fileId}`}>
      <source
        src={`${API_BASE_URL}/media/stream?id=${fileId}`}
        type="video/mp4"
      />
      Your browser does not support the video tag.
    </video>
  );
};

// eslint-disable-next-line @typescript-eslint/no-unused-vars
const statuses = ["init", "loading", "success", "error"] as const;
type Status = (typeof statuses)[number];

const mediaFilesFilter = (item: GlobalState["files"][string][number]) =>
  item.type === "file" &&
  (item.mime_type?.startsWith("image/") ||
    item.mime_type?.startsWith("video/"));

export default function HomeScreen() {
  const { fileId, pathId } = useLocalSearchParams<{
    fileId: string;
    pathId: string;
  }>();
  const { globalState, setGlobalState } = useContext(GlobalContext);
  const status = useRef<Status>("init");
  const mediaFiles = globalState.files?.[pathId]?.filter(mediaFilesFilter);
  const fileIndex =
    mediaFiles?.findIndex((item) => `${item.id}` === fileId) ?? 0;
  const fileData = mediaFiles?.[fileIndex];
  const { width: windowWidth, height: windowHeight } = Dimensions.get("window");
  const snapContainerRef = useRef<HTMLDivElement | null>(null);
  const [activeItemId, setActiveItemId] = useState<number>(fileData?.id ?? -1);
  const [isScrollSnapChangingSupported, setIsScrollSnapChangingSupported] =
    useState(false);

  useEffect(() => {
    setIsScrollSnapChangingSupported(
      "onscrollsnapchanging" in document?.createElement("div")
    );
  }, []);

  useEffect(() => {
    snapContainerRef?.current?.scroll(0, fileIndex * windowHeight);
  }, []);

  useEffect(() => {
    if (fileData) {
      return;
    }

    const url = new URL("/media/details", API_BASE_URL);
    url.searchParams.append("path", fileId);

    const options = {
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
          files: { ...prev.files, [pathId]: [json] },
        }));
        status.current = "success";
      })
      .catch((err) => {
        console.error("error:" + err);
        status.current = "error";
      });
  }, [fileId, pathId, fileData, setGlobalState]);

  const elem = mediaFiles?.map((item, index) => {
    let viewer = null;
    if (item.kind === "image") {
      viewer = <ImageViewer fileId={`${item.id}`} />;
    } else if (item.kind === "video") {
      viewer = <VideoPlayer fileId={`${item.id}`} />;
    }

    return (
      <div
        key={item.name}
        style={{
          contain: "strict",
          height: windowHeight,
          width: windowWidth,
          justifyContent: "center",
          position: "absolute",
          top: index * windowHeight,
          display: "flex",
          flexDirection: "column",
        }}
        data-viewer-id={item.id}
      >
        {fileIndex === index ||
        index === fileIndex + 1 ||
        index === fileIndex - 1
          ? viewer
          : null}
      </div>
    );
  });

  const router = useRouter();
  const [headerShown, setHeaderShown] = useState(true);

  useEffect(() => {
    const temp = snapContainerRef.current;
    const handler = (e: Event) => {
      const newId = (e as Event & { snapTargetBlock: HTMLElement })
        .snapTargetBlock.dataset.viewerId;
      if (newId) {
        router.setParams({ fileId: newId });
        setActiveItemId(parseInt(newId));
      }
    };
    temp?.addEventListener("scrollsnapchanging", handler);
    return () => {
      temp?.removeEventListener("scrollsnapchanging", handler);
    };
  });

  if (!fileData) {
    return null;
  }

  return (
    <Pressable
      onPress={() => setHeaderShown((prev) => !prev)}
      style={{ flex: 1 }}
    >
      <div
        ref={snapContainerRef}
        className={styles.testView}
        style={{ overflowY: "scroll", position: "relative", flex: 1 }}
        onScroll={(e) => {
          if (isScrollSnapChangingSupported) {
            return;
          }
          const target = e.target as HTMLDivElement;
          const scrollTop = target.scrollTop;
          const newIndex = Math.round(scrollTop / windowHeight);
          const newId = mediaFiles?.[newIndex]?.id;
          if (newId && newId !== activeItemId) {
            router.setParams({ fileId: `${newId}` });
            setActiveItemId(newId);
          }
        }}
      >
        <Stack.Screen
          options={{
            title:
              mediaFiles?.find((item) => item.id === activeItemId)?.name ??
              fileData.name ??
              "Files",
            headerShown,
          }}
        />
        {elem}
      </div>
    </Pressable>
  );
}
