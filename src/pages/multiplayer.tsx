import {
  Alert,
  AlertIcon,
  AlertTitle,
  Badge,
  Box,
  Button,
  Center,
  FormControl,
  FormLabel,
  HStack,
  IconButton,
  Input,
  Progress,
  SimpleGrid,
  Spinner,
  Text,
  Tooltip,
  VStack,
} from "@chakra-ui/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  LuArrowLeft,
  LuCopy,
  LuDownload,
  LuLogIn,
  LuPlay,
  LuRefreshCw,
  LuSquare,
  LuUsers,
} from "react-icons/lu";
import { Section } from "@/components/common/section";
import { useGlobalData } from "@/contexts/global-data";
import { useToast } from "@/contexts/toast";
import { TerracottaState } from "@/models/terracotta";
import { TerracottaService } from "@/services/terracotta";
import { copyText } from "@/utils/copy";

const MultiplayerPage = () => {
  const { t } = useTranslation();
  const toast = useToast();
  const { selectedPlayer } = useGlobalData();
  const playerName = selectedPlayer?.name ?? "";
  const [state, setState] = useState<TerracottaState>();
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const actionInFlight = useRef(false);
  const [roomCode, setRoomCode] = useState("");
  const [mode, setMode] = useState<"choose" | "host" | "join">("choose");

  const refresh = useCallback(async () => {
    const response = await TerracottaService.getState();
    if (response.status === "success") setState(response.data);
    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (state && !state.running) {
      setMode("choose");
      return;
    }
    if (state?.status.startsWith("host-")) setMode("host");
    if (state?.status.startsWith("guest-")) setMode("join");
  }, [state]);

  const runAction = async (
    action: () => Promise<{ status: string; details?: string }>
  ) => {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    setBusy(true);
    try {
      const response = await action();
      if (response.status === "error") {
        toast({
          status: "error",
          title: response.details || t("TerracottaPage.error"),
        });
      } else {
        await refresh();
      }
    } finally {
      actionInFlight.current = false;
      setBusy(false);
    }
  };

  const validRoomCode = /^U\/[A-Z0-9]{4}(?:-[A-Z0-9]{4}){3}$/i.test(
    roomCode.trim()
  );
  const sessionInProgress = [
    "host-scanning",
    "host-starting",
    "guest-connecting",
    "guest-starting",
  ].includes(state?.status ?? "");
  const sessionReady = [
    "host-ok",
    "guest-ok",
    "host-ready",
    "guest-ready",
  ].includes(state?.status ?? "");
  if (loading) {
    return (
      <Center h="100%">
        <Spinner />
      </Center>
    );
  }

  return (
    <Section
      className="content-full-y"
      title={t("TerracottaPage.title")}
      withBackButton
    >
      <VStack
        align="stretch"
        spacing={{ base: 4, md: 6 }}
        maxW="64rem"
        mx="auto"
        px={{ base: 2, md: 8 }}
      >
        <HStack justify="space-between" align="center">
          <HStack>
            <LuUsers />
            <Text fontWeight="bold" fontSize={{ base: "md", md: "lg" }}>
              Terracotta
            </Text>
            <Badge
              colorScheme={state?.running ? "green" : "gray"}
              variant="subtle"
              borderRadius="full"
              px={2}
            >
              {state?.running
                ? t("TerracottaPage.running")
                : t("TerracottaPage.stopped")}
            </Badge>
          </HStack>
          <Button
            size="sm"
            variant="ghost"
            leftIcon={<LuRefreshCw />}
            onClick={() => void refresh()}
            isLoading={busy}
          >
            {t("TerracottaPage.refresh")}
          </Button>
        </HStack>

        <HStack spacing={2} width="full" flexWrap="nowrap">
          {!state?.installed ? (
            <Button
              flex={1}
              minW={0}
              fontSize={{ base: "xs", md: "sm" }}
              leftIcon={<LuDownload />}
              onClick={() => void runAction(() => TerracottaService.download())}
              isLoading={busy}
            >
              {t("TerracottaPage.download")}
            </Button>
          ) : (
            <>
              <Button
                flex={1}
                minW={0}
                fontSize={{ base: "xs", md: "sm" }}
                leftIcon={<LuPlay />}
                onClick={() => void runAction(() => TerracottaService.start())}
                isLoading={busy}
                isDisabled={sessionInProgress || sessionReady}
              >
                {t("TerracottaPage.start")}
              </Button>
              <Button
                flex={1}
                minW={0}
                fontSize={{ base: "xs", md: "sm" }}
                leftIcon={<LuDownload />}
                onClick={() => void runAction(() => TerracottaService.update())}
                isLoading={busy}
                isDisabled={!state.updateAvailable}
              >
                {t("TerracottaPage.update")}{" "}
                {state.latestVersion ? `(${state.latestVersion})` : ""}
              </Button>
            </>
          )}
          {state?.running && (
            <Button
              flex={1}
              minW={0}
              fontSize={{ base: "xs", md: "sm" }}
              leftIcon={<LuSquare />}
              colorScheme="red"
              variant="outline"
              onClick={() => void runAction(() => TerracottaService.stop())}
              isLoading={busy}
            >
              {t("TerracottaPage.stop")}
            </Button>
          )}
        </HStack>

        {typeof state?.downloadProgress === "number" && (
          <VStack align="stretch" spacing={2}>
            <Text fontSize="sm" className="secondary-text">
              {t("TerracottaPage.downloadProgress", {
                progress: state.downloadProgress,
              })}
            </Text>
            <Progress
              value={state.downloadProgress}
              max={100}
              size="sm"
              borderRadius="sm"
            />
          </VStack>
        )}

        {state?.running && sessionReady && (
          <Box
            borderWidth="1px"
            borderColor="green.400"
            borderRadius="md"
            p={{ base: 5, md: 7 }}
          >
            <VStack align="stretch" spacing={4}>
              <HStack>
                <Badge colorScheme="green" borderRadius="full" px={2}>
                  {t("TerracottaPage.inRoom")}
                </Badge>
                <Text fontWeight="bold">
                  {state.status.startsWith("host-")
                    ? t("TerracottaPage.host")
                    : t("TerracottaPage.join")}
                </Text>
              </HStack>
              {state.roomCode && (
                <Box>
                  <Text fontSize="xs" opacity={0.7} textTransform="uppercase">
                    {t("TerracottaPage.roomCode")}
                  </Text>
                  <HStack mt={1} spacing={1}>
                    <Text
                      fontFamily="mono"
                      fontSize={{ base: "lg", md: "xl" }}
                      fontWeight="bold"
                    >
                      {state.roomCode}
                    </Text>
                    <Tooltip label={t("TerracottaPage.copyRoomCode")}>
                      <IconButton
                        aria-label={t("TerracottaPage.copyRoomCode")}
                        icon={<LuCopy />}
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                          void copyText(state.roomCode ?? "", { toast })
                        }
                      />
                    </Tooltip>
                  </HStack>
                </Box>
              )}
              {state.serverAddress && (
                <Text fontSize="sm" className="secondary-text" noOfLines={1}>
                  {t("TerracottaPage.serverAddressValue", {
                    address: state.serverAddress,
                  })}
                </Text>
              )}
              <Box>
                <Text fontWeight="bold" fontSize="sm" mb={2}>
                  {t("TerracottaPage.players")}
                </Text>
                {state.players?.length ? (
                  <VStack align="stretch" spacing={1}>
                    {state.players.map((player) => (
                      <HStack
                        key={`${player.machineId}-${player.name}`}
                        justify="space-between"
                        px={3}
                        py={2}
                        borderRadius="sm"
                        bg="whiteAlpha.100"
                      >
                        <Text fontSize="sm" noOfLines={1}>
                          {player.name}
                        </Text>
                        {player.kind && (
                          <Badge size="sm" variant="subtle">
                            {player.kind}
                          </Badge>
                        )}
                      </HStack>
                    ))}
                  </VStack>
                ) : (
                  <Text fontSize="sm" className="secondary-text">
                    {t("TerracottaPage.noPlayers")}
                  </Text>
                )}
              </Box>
              <Button
                alignSelf="flex-start"
                leftIcon={<LuSquare />}
                colorScheme="red"
                variant="outline"
                onClick={() =>
                  void runAction(async () => {
                    const response = await TerracottaService.closeRoom();
                    if (response.status === "success") setMode("choose");
                    return response;
                  })
                }
                isLoading={busy}
              >
                {t("TerracottaPage.closeRoom")}
              </Button>
            </VStack>
          </Box>
        )}

        {state?.running && sessionInProgress && (
          <Alert status="info" borderRadius="md" alignItems="flex-start">
            <AlertIcon mt={1} />
            <Box>
              <AlertTitle>{t("TerracottaPage.connecting")}</AlertTitle>
              {mode === "host" && (
                <Text fontSize="sm">{t("TerracottaPage.lanHint")}</Text>
              )}
            </Box>
          </Alert>
        )}

        {state?.running &&
          !sessionReady &&
          !sessionInProgress &&
          mode === "choose" && (
            <SimpleGrid columns={{ base: 1, sm: 2 }} spacing={4}>
              <Button
                h={{ base: "5rem", md: "6rem" }}
                variant="outline"
                leftIcon={<LuUsers />}
                onClick={() => setMode("host")}
              >
                {t("TerracottaPage.host")}
              </Button>
              <Button
                h={{ base: "5rem", md: "6rem" }}
                variant="outline"
                leftIcon={<LuLogIn />}
                onClick={() => setMode("join")}
              >
                {t("TerracottaPage.join")}
              </Button>
            </SimpleGrid>
          )}

        {state?.running &&
          !sessionReady &&
          !sessionInProgress &&
          mode !== "choose" && (
            <Box
              borderWidth="1px"
              borderColor="whiteAlpha.200"
              borderRadius="md"
              p={{ base: 4, md: 6 }}
            >
              <VStack align="stretch" spacing={4}>
                <Button
                  alignSelf="flex-start"
                  size="sm"
                  variant="ghost"
                  leftIcon={<LuArrowLeft />}
                  onClick={() => setMode("choose")}
                >
                  {t("TerracottaPage.back")}
                </Button>
                <Text fontSize="lg" fontWeight="bold">
                  {t(
                    mode === "host"
                      ? "TerracottaPage.host"
                      : "TerracottaPage.join"
                  )}
                </Text>
                {mode === "host" && (
                  <Alert
                    status="info"
                    borderRadius="md"
                    alignItems="flex-start"
                  >
                    <AlertIcon mt={1} />
                    <Box>
                      <AlertTitle>
                        {t("TerracottaPage.openLanTitle")}
                      </AlertTitle>
                      <Text fontSize="sm">{t("TerracottaPage.lanHint")}</Text>
                    </Box>
                  </Alert>
                )}
                {mode === "join" && (
                  <FormControl>
                    <FormLabel>{t("TerracottaPage.roomCode")}</FormLabel>
                    <Input
                      size="sm"
                      value={roomCode}
                      placeholder="U/XXXX-XXXX-XXXX-XXXX"
                      onChange={(e) =>
                        setRoomCode(e.target.value.toUpperCase())
                      }
                    />
                  </FormControl>
                )}
                <Button
                  width="full"
                  colorScheme="green"
                  onClick={() =>
                    void runAction(() =>
                      mode === "host"
                        ? TerracottaService.host(playerName)
                        : TerracottaService.join(playerName, roomCode.trim())
                    )
                  }
                  isLoading={busy}
                  isDisabled={
                    !playerName.trim() || (mode === "join" && !validRoomCode)
                  }
                >
                  {t(
                    mode === "host"
                      ? "TerracottaPage.host"
                      : "TerracottaPage.join"
                  )}
                </Button>
              </VStack>
            </Box>
          )}
      </VStack>
    </Section>
  );
};

export default MultiplayerPage;
