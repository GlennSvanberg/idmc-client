<!-- source: REST_API_versions_of_the_login_resource.html -->
# REST API versions of the login resource

# REST API versions of the login resource

You can log in to IDMC using the platform REST API version 2 or version 3 login resource.

**Note:** 

The version of any subsequent resource that you use does not need to match the version of the login resource that you use to log in.

Note the following differences between REST API version 2 and version 3 calls:

Format

You can use the following formats depending upon which API version you use:

* Version 2 supports XML and JSON calls.
* Version 3 supports JSON calls.

Login URL

Use one of the following login URLs:

* For version 2, use https://dm-<POD region>.informaticacloud.com/ma/api/v2/user/login.
* For version 3, use https://dm-<POD region>.informaticacloud.com/saas/public/core/v3/login.

Your POD (Point of Deployment) region is based on the location of your IDMC data center. The POD region is included in the URL that you received when you registered with IDMC. For North America, use us.

Base URL

The login response includes the base URL that you must include in subsequent calls. The base URL includes the following components:

* The name and region of the POD that your organization uses, for example, use4-mdm.dm-us.
* The IDMC domain, informaticacloud.com.
* The internal service that manages the API calls, for example, saas.

The following example is a base URL for an organization on the use4-mdm.dm-us POD:

https://use4-mdm.dm-us.informaticacloud.com/saas

In the login response, the attribute that provides the base URL depends on the API version that you use to log in. For example:

* In a version 2 response, the attribute name is serverUrl.
* In a version 3 response, the attribute name is baseApiUrl.

Session ID

The login response includes a session ID that you must include in headers for subsequent requests during this session. In the login response, the name of the session ID attribute depends on the API version that you used to log in.

Use one of the following attributes:

* For version 2, use icSessionId in the header.
* For version 3, use sessionId in the header.

For more information about session IDs, see [Session IDs and JSON Web Tokens](Session_IDs_and_JSON_Web_Tokens.md "Session IDs and JSON Web Tokens")