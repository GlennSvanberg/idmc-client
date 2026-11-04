<!-- source: Session_IDs_and_JSON_Web_Tokens.html -->
# Session IDs and JSON Web Tokens

# Session IDs and JSON Web Tokens

When you log in to an IDMC organization using the REST API, the login resource returns a session ID or JSON Web Token (JWT). You include this session ID or JWT token in subsequent REST API requests during the session.

You can configure the organization to use one of following authentication types:

* Session ID. A session ID expires after 30 minutes of inactivity. A grace period of up to 2 minutes might be included to account for any network or unexpected delays.
* JSON Web Token (JWT). A JWT token expires based on the expiration time set by an administrator. If you use custom scripts, ensure that the scripts handle automatic re-authentication before the JWT token expires to avoid any interruptions. For more information, see the [JWT Support](https://knowledge.informatica.com/s/article/JWT-Support) Knowledge article.

If your session ID or JWT token expires, you can log in again to get a new value.

To get a session ID or JWT token, submit a POST request with your credentials. For more information, see [Authentication method](Authentication_method.md "Authentication method").

If you use session-based authentication, the response returns a session ID. For example, you might receive the following response when you log in using Login API V3:

{  
 "products": [  
 {  
 "name": "Integration Cloud",  
 "baseApiUrl": "https://qa-pod1.rel.infaqa.com/saas"  
 }  
 ],  
 "userInfo": {  
 "sessionId": "2hJtHrszOGjloYpFNYdmic",  
 "id": "ke4eMsBKKmwh978SIbFyaT",  
 "name": "rel\_ebf2",  
 "parentOrgId": "52ZSTB0IDK6dXxaEQLUaQu",  
 "orgId": "5ONW1IYW5L6ljpkkUqfBGK",  
 "orgName": "rel\_ebf2",  
 "groups": {},  
 "status": "Active"  
 }  
}

If you use JWT-based authentication, the response returns a JWT token. For example, you might receive the following response when you log in using Login API V3:

{  
 "products": [  
 {  
 "name": "Integration Cloud",  
 "baseApiUrl": "https://qa-pod1.rel.infaqa.com/saas"  
 }  
 ],  
 "userInfo": {  
 "sessionId": "eyJraWQiOiJnOElCd3pEd1hVc2owRHQ4RjJQVjhjIiwidHlwIjoiSldUIiwiYWxnIjoiRVMyNTYifQ.eyJyZWxlYXNlX2lkIjoiIiwiY3JfaWQiOiI4MGZrSTdlT2IxWGV4aTJVNnUyTjBRIiwidXNlcl9uYW1lIjoicmVsX2ViZjIiLCJpc3MiOiJodHRwczovL3FhLW1hLnJlbC5pbmZhcWEuY29tL2lkZW50aXR5LXNlcnZpY2UiLCJjdXJyZW50X29yZ19pZCI6IjVPTlcxSVlXNUw2bGpwa2tVcWZCR0siLCJkZXBsb3ltZW50R3JvdXBJZCI6IjE5TFZwOHJ1VjZKZlM5MTRKR3hlNjEiLCJ1dHlwZSI6MCwiY2xpZW50X2lkIjoiaWRtY19hcHAiLCJ1c2VyX29yZ19pZCI6IjVPTlcxSVlXNUw2bGpwa2tVcWZCR0siLCJ1c3QiOjEsImNvbnRleHRfdHlwZSI6InVzZXItd2ViLWFwcCIsImV4cCI6MTc3NTIzNTcwMCwib3JnX25hbWUiOiJyZWxfZWJmMiIsInRpbWVfem9uZV9pZCI6IkFtZXJpY2EvTG9zX0FuZ2VsZXMiLCJpYXQiOjE3NzUyMzM5MDAsImp0aSI6ImRETUJlMllFbVEwYjR2SXhkNUN0bzEiLCJlZmZlY3RpdmVfcHJpdmlsZWdlc19sdyI6IlBMZS9sZC8vaytzM3VaZTMvMC9OL25uOTlYL2Q3WS8vLy84dkFBREFBQUFjQVFDLy8vLy8vLy8vLy8vLy8vLy8rNy8vLy8zdmYrOER4UC8vLy8vLy94MEFBdmgvRGdBQUFBQUFnUEgvQVFCNG51ZDVudWQ1QU9BQkJ3QUFBQUFBQUFBQUFBQWd3RThBQVArL2tlZjQvMy80L3YvLy8vLy8vd0VmQVB3QUFBQUFBQUFBQUFBQUFBQUFBUEFIbnVmLy8vK0ZIL0QvLy8vL1h4RUFCdkRoQndMeS8vL1A4enhQL2kvLy8vOGJBQUFBL0IvL0J3QUFHaU1JLy8vMy8vLy93d01EQVB3LyIsImVmZmVjdGl2ZV9yb2xlcyI6eyJEZXNpZ25lciI6IjFWZm5zZ1ppQ1QxZmkyNVZBdXBRZzEiLCJEYXRhIFByZXZpZXciOiI5YzJYcmRwQXo4MGhnMjl5WERCUEVOIiwiQWRtaW4iOiI5Z2VkQkRvWVFvUWliTk1vaGY1S0NoIn0sImN1cnJlbnRfb3JnX25hbWUiOiJyZWxfZWJmMiIsInVuaXF1ZV9pZCI6IiQyYSQwNyQ4NWtXTDM3UjNSQ2VIRzFKRDBVM3kuZXM2aElkcUZVSGFQSGp5UTVKazY4M1NiLnBZVHU4QyIsInVzZXJfZm5hbWUiOiJwcmFzYW5uYSIsImdyb3VwcyI6e30sIm5vbmNlIjoiNXNiMm1hZjVkdzNpdzhZS1dXRWxmZyIsInVzZXJfbG5hbWUiOiJrdW1hciIsImF1ZCI6ImlkbWNfYXBwIiwidXNlcl9pZCI6ImtlNGVNc0JLS213aDk3OFNJYkZ5YVQiLCJvcmdfaWQiOiI1T05XMUlZVzVMNmxqcGtrVXFmQkdLIiwic3NfYXRyIjp7fSwicGFyZW50X29yZ19pZCI6IjUyWlNUQjBJREs2ZFh4YUVRTFVhUXUifQ.nI8WcJDIMflXI000WAimwffLyZUcMUhET451v7mGuZxrHTdNyy0fPVgJPaRhj5JXv\_lZpMakFlk2psMoe8yyHg",  
 "id": "ke4eMsBKKmwh978SIbFyaT",  
 "name": "rel\_ebf2",  
 "parentOrgId": "52ZSTB0IDK6dXxaEQLUaQu",  
 "orgId": "5ONW1IYW5L6ljpkkUqfBGK",  
 "orgName": "rel\_ebf2",  
 "groups": {},  
 "status": "Active"  
 }  
}

To make a request, add the session ID or JWT token you retrieved to the IDS-SESSION-ID header in the request. Ensure that you use only the IDS-SESSION-ID header for authentication, not the Authorization header.

The following example shows how to use a retrieved session ID in the request header:

POST https://use4.dm-us.informaticacloud.com/search/public/api/v1/search \  
 --header 'Accept: application/json \  
 --header 'Content-Type: application/json \  
 --header 'IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX \  
   
{  
 "entityType": "Example Entity"  
  
 }