import {
  DarkTheme,
  DefaultTheme,
  ThemeProvider,
} from "@react-navigation/native";
import { useFonts } from "expo-font";
import { Stack } from "expo-router";
import Head from "expo-router/head";
import { StatusBar } from "expo-status-bar";
import "react-native-reanimated";
import { Colors } from "@/constants/Colors";

import { useColorScheme } from "@/hooks/useColorScheme";
import { GlobalContextProvider } from "@/components/GlobalContext";

export default function RootLayout() {
  const colorScheme = useColorScheme();
  const [loaded] = useFonts({
    SpaceMono: require("../assets/fonts/SpaceMono-Regular.ttf"),
  });

  if (!loaded) {
    // Async font loading only occurs in development.
    return null;
  }

  return (
    <GlobalContextProvider>
      <ThemeProvider value={colorScheme === "dark" ? DarkTheme : DefaultTheme}>
        <Head>
          <style>{`
          body {
            background-color: ${
              colorScheme === "dark"
                ? Colors.dark.background
                : Colors.light.background
            };
          }
        `}</style>
        </Head>
        <Stack>
          <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
          <Stack.Screen name="+not-found" />
          <Stack.Screen
            name="video/[fileId]"
            options={{ headerTransparent: true }}
          />
          <Stack.Screen
            name="image/[fileId]"
            options={{ headerTransparent: true }}
          />
        </Stack>
        <StatusBar style="auto" />
      </ThemeProvider>
    </GlobalContextProvider>
  );
}

export const unstable_settings = {
  // Ensure any route can link back to `/`
  initialRouteName: "(tabs)",
};
