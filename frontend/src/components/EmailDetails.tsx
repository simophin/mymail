import {AccountId, BodyPart, Email} from "./ThreadList";
import {createEffect, createMemo, createResource, createSignal, Match, Show, Switch} from "solid-js";
import * as zod from "zod";
import {log as parentLog} from "../log";

const apiUrl: string = import.meta.env.VITE_BASE_URL;

const log = parentLog.child({ "component": "EmailDetails" });

export default function EmailDetails(props: {
    accountId: AccountId,
    email: Email,
    defaultAllowRemoteContent?: boolean
}) {
    const [allowRemoteContent, setAllowRemoteContent] = createSignal(props.defaultAllowRemoteContent == true);
    const [loadedIframe, setLoadedIframe] = createSignal<HTMLIFrameElement | null>(null);

    createEffect(() => {
        const iframe = loadedIframe();
        iframe?.contentDocument?.querySelectorAll('img').forEach(img => {
            const src = img.src;
            if (src && src.startsWith('cid:')) {
                const cid = src.substring(4);
                const realLink = getPartUrl(props.email.attachments ?? [], (part) => part.cid === cid);
                if (realLink) {
                    img.src = realLink;
                } else {
                    img.removeAttribute('src');
                }
            } else if (src) {
                img.src = `${apiUrl}/proxy?url=${encodeURIComponent(src)}`;
            }
        });
    })

    const getPartUrl = (parts: BodyPart[], search: (part: BodyPart) => boolean) => {
        const part = parts.find(search);
        if (part) {
            const mimeType = (!part.type.startsWith("text/") || part.type.includes("charset")) ? part.type : `${part.type}; charset=utf-8`;
            return `${apiUrl}/blobs/${props.accountId}/${part.blobId}?mimeType=${encodeURIComponent(mimeType)}`;
        }
    };

    const mainHtmlBodyUrl = createMemo(() => {
        return getPartUrl(props.email.htmlBody, (part) => part.type.startsWith("text/html")) + '&sanitizeHtml=true';
    });

    const mainTextBodyUrl = createMemo(() => {
        return getPartUrl(props.email.textBody, (part) => part.type.startsWith("text/plain"));
    });

    const onPageLoaded = (event: Event) => {
        const iframe = event.target as HTMLIFrameElement;
        setLoadedIframe(iframe);

        iframe.contentDocument?.querySelectorAll('a').forEach((link) => {
            link.target = "_blank";
        });
    };

    return <>
        <Switch fallback={"No text available"}>
            <Match when={!!mainHtmlBodyUrl()}>
                <iframe class="w-full h-full overflow-scroll"
                        src={mainHtmlBodyUrl()}
                        onload={onPageLoaded}
                        sandbox="allow-popups allow-same-origin"/>
            </Match>

            <Match when={!!mainTextBodyUrl()}>
                <iframe class="w-full h-full overflow-scroll" src={mainTextBodyUrl()} sandbox=""/>
            </Match>
        </Switch>
    </>
}