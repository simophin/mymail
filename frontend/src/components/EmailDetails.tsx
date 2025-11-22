import {AccountId, Email} from "./ThreadList";
import {createResource, Match, Switch} from "solid-js";
import * as zod from "zod";

const apiUrl: string = import.meta.env.VITE_BASE_URL;

const EmailDetailsSchema = zod.object({
    "bodyValues": zod.record(zod.string(), zod.object({})),
});

type EmailDetails = zod.infer<typeof EmailDetailsSchema>;

export default function EmailDetails(props: {
    accountId: AccountId,
    email: Email,
}) {
    const [details, {refetch}] = createResource(async () => {
        const resp = await fetch(`${apiUrl}/mails/${props.accountId}/${props.email.id}`);
        return zod.parse(EmailDetailsSchema, await resp.json());
    });

    return <Switch>
        <Match when={details.state === 'pending'}>
            <div class="loading loading-bars loading-md"></div>
        </Match>
    </Switch>
}