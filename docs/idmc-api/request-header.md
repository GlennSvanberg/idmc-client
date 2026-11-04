<!-- source: Request_header.html -->
# Request header

# Request header

Use a request header to define the operating parameters or the metadata of a REST operation.

The following table lists the request header parameters:

| Parameter | Description |
| --- | --- |
| Content-Type | Format of the request body. |
| Accept | Format of the request response. Accepts application/json. |
| IDS-SESSION-ID | The session ID of the user. See [Session IDs and JSON Web Tokens](Session_IDs_and_JSON_Web_Tokens.md "Session IDs and JSON Web Tokens"). |
| MODEL\_VERSION | Current version of the MDM SaaS configuration that includes the data model. Specify the model version as the current epoch timestamp in milliseconds.  If you don't specify the model version, the REST API defaults to using the cached version. Metadata related to the model is refreshed and cached every five minutes. If you submit API requests immediately after significant model changes without specifying the model version, responses might be based on an outdated cached model version. To get a correct response, specify the model version or wait up to five minutes until the metadata related to model refreshes and caches before you submit requests.  To improve performance and avoid request failures due to version differences, specify the same model version parameter in the request header when you send multiple requests in succession. |
| Accept-Language | Optional. Preferred languages for localized picklist field values and messages in the response. Enter one or more BCP 47 language codes in order of preference. You can assign a quality value to each language tag to set how strongly you prefer it. For example, fr-FR,en;q=0.8. |

To construct the REST API request header for a resource, add the header parameter after the following request line:

//<baseURL>/v1/api/

Use the following syntax to construct the REST API request header for a REST API resource:

<METHOD> https: //<baseApiURL>/business-entity/public/api/v1/<resource name>  
Content-Type: application/json   
Accept: application/json  
Accept-Language: <language-tag>,<language-tag>;q=<value>  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX

For example, you might use the following request to read a business entity master record:

GET https: //<baseApiURL>/business-entity/public/api/v1/entity/c360.organization/70046899550283630729399822719 HTTP/1.1   
Content-Type: application/json   
Accept: application/json  
Accept-Language: fr-FR,en;q=0.8   
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX  
MODEL\_VERSION:1706774113534