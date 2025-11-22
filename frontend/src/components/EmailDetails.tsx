import {Email} from "./ThreadList";

export default function EmailDetails(props: {
    email: Email,
}) {
    return <div>
        {Object.keys(props.email.bodyValues ?? {}).length} body values
    </div>
}